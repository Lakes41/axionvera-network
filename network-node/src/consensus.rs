use crate::error::{NetworkError, Result};
use crate::telemetry::{extract_traceparent_grpc, inject_traceparent_grpc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, instrument, warn, Span};
use uuid::Uuid;

/// Consensus vote type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteType {
    Approve,
    Reject,
    Abstain,
}

/// Consensus proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub proposer: String,
    pub content: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub required_votes: usize,
    pub current_votes: usize,
    pub status: ProposalStatus,
}

impl Proposal {
    pub fn new(
        proposer: String,
        content: Vec<u8>,
        required_votes: usize,
        ttl_minutes: u64,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            proposer,
            content,
            created_at: now,
            expires_at: now + chrono::Duration::minutes(ttl_minutes as i64),
            required_votes,
            current_votes: 0,
            status: ProposalStatus::Active,
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }

    pub fn can_vote(&self) -> bool {
        matches!(self.status, ProposalStatus::Active) && !self.is_expired()
    }

    pub fn has_quorum(&self) -> bool {
        self.current_votes >= self.required_votes
    }
}

/// Proposal status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalStatus {
    Active,
    Approved,
    Rejected,
    Expired,
}

/// Vote record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub proposal_id: String,
    pub voter: String,
    pub vote_type: VoteType,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub signature: Vec<u8>,
    pub trace_context: Option<String>,
}

impl Vote {
    pub fn new(
        proposal_id: String,
        voter: String,
        vote_type: VoteType,
        signature: Vec<u8>,
    ) -> Self {
        Self {
            proposal_id,
            voter,
            vote_type,
            timestamp: chrono::Utc::now(),
            signature,
            trace_context: None,
        }
    }

    pub fn with_trace_context(mut self, trace_context: String) -> Self {
        self.trace_context = Some(trace_context);
        self
    }
}

/// Consensus engine
pub struct ConsensusEngine {
    node_id: String,
    proposals: Arc<RwLock<HashMap<String, Proposal>>>,
    votes: Arc<RwLock<HashMap<String, Vec<Vote>>>>,
    vote_sender: mpsc::UnboundedSender<Vote>,
    proposal_sender: mpsc::UnboundedSender<Proposal>,
    required_votes: usize,
    proposal_ttl_minutes: u64,
}

impl ConsensusEngine {
    /// Create a new consensus engine
    pub fn new(
        node_id: String,
        required_votes: usize,
        proposal_ttl_minutes: u64,
    ) -> (Self, mpsc::UnboundedReceiver<Vote>, mpsc::UnboundedReceiver<Proposal>) {
        let (vote_sender, vote_receiver) = mpsc::unbounded_channel();
        let (proposal_sender, proposal_receiver) = mpsc::unbounded_channel();

        let engine = Self {
            node_id,
            proposals: Arc::new(RwLock::new(HashMap::new())),
            votes: Arc::new(RwLock::new(HashMap::new())),
            vote_sender,
            proposal_sender,
            required_votes,
            proposal_ttl_minutes,
        };

        (engine, vote_receiver, proposal_receiver)
    }

    /// Create a new proposal
    #[instrument(skip(self, content), fields(node_id = %self.node_id, content_size = content.len()))]
    pub async fn create_proposal(&self, content: Vec<u8>) -> Result<Proposal> {
        let proposal = Proposal::new(
            self.node_id.clone(),
            content,
            self.required_votes,
            self.proposal_ttl_minutes,
        );

        info!(
            "Creating proposal {} with {} required votes",
            proposal.id,
            proposal.required_votes
        );

        // Store proposal
        {
            let mut proposals = self.proposals.write().await;
            proposals.insert(proposal.id.clone(), proposal.clone());
        }

        // Broadcast proposal
        if let Err(e) = self.proposal_sender.send(proposal.clone()) {
            error!("Failed to broadcast proposal {}: {}", proposal.id, e);
            return Err(NetworkError::Server(format!("Failed to broadcast proposal: {}", e)));
        }

        info!("Proposal {} created and broadcasted", proposal.id);
        Ok(proposal)
    }

    /// Vote on a proposal
    #[instrument(skip(self, signature), fields(node_id = %self.node_id, proposal_id, vote_type = ?vote_type))]
    pub async fn vote(
        &self,
        proposal_id: &str,
        vote_type: VoteType,
        signature: Vec<u8>,
    ) -> Result<()> {
        let span = tracing::info_span!(
            "consensus_vote",
            node_id = %self.node_id,
            proposal_id = %proposal_id,
            vote_type = ?vote_type,
            voter = %self.node_id
        );
        let _enter = span.enter();

        info!("Casting vote on proposal: {}", proposal_id);

        // Check if proposal exists and is active
        let proposal = {
            let proposals = self.proposals.read().await;
            proposals.get(proposal_id).cloned()
        };

        let proposal = match proposal {
            Some(p) => p,
            None => {
                warn!("Proposal {} not found", proposal_id);
                return Err(NetworkError::Validation(format!("Proposal {} not found", proposal_id)));
            }
        };

        if !proposal.can_vote() {
            warn!("Proposal {} is not available for voting", proposal_id);
            return Err(NetworkError::Validation(format!("Proposal {} is not active", proposal_id)));
        }

        // Check if already voted
        {
            let votes = self.votes.read().await;
            if let Some(proposal_votes) = votes.get(proposal_id) {
                if proposal_votes.iter().any(|v| v.voter == self.node_id) {
                    warn!("Already voted on proposal: {}", proposal_id);
                    return Err(NetworkError::Validation(format!("Already voted on proposal {}", proposal_id)));
                }
            }
        }

        // Create vote
        let vote = Vote::new(
            proposal_id.to_string(),
            self.node_id.clone(),
            vote_type,
            signature,
        );

        // Store vote
        {
            let mut votes = self.votes.write().await;
            let proposal_votes = votes.entry(proposal_id.to_string()).or_insert_with(Vec::new);
            proposal_votes.push(vote.clone());
        }

        // Update proposal vote count
        {
            let mut proposals = self.proposals.write().await;
            if let Some(proposal) = proposals.get_mut(proposal_id) {
                proposal.current_votes += 1;
                
                // Check if quorum is reached
                if proposal.has_quorum() {
                    self.finalize_proposal(proposal_id).await?;
                }
            }
        }

        // Broadcast vote
        if let Err(e) = self.vote_sender.send(vote) {
            error!("Failed to broadcast vote for proposal {}: {}", proposal_id, e);
            return Err(NetworkError::Server(format!("Failed to broadcast vote: {}", e)));
        }

        info!("Vote cast successfully on proposal: {}", proposal_id);
        Ok(())
    }

    /// Process incoming vote from another node
    #[instrument(skip(self, vote), fields(node_id = %self.node_id, proposal_id = %vote.proposal_id, voter = %vote.voter))]
    pub async fn process_vote(&self, vote: Vote) -> Result<()> {
        info!("Processing vote from {} for proposal: {}", vote.voter, vote.proposal_id);

        // Extract trace context if present
        if let Some(trace_context) = &vote.trace_context {
            debug!("Extracting trace context from vote: {}", trace_context);
            // In a real implementation, you would restore the trace context here
        }

        // Check if proposal exists
        let proposal = {
            let proposals = self.proposals.read().await;
            proposals.get(&vote.proposal_id).cloned()
        };

        let proposal = match proposal {
            Some(p) => p,
            None => {
                warn!("Received vote for unknown proposal: {}", vote.proposal_id);
                return Err(NetworkError::Validation(format!("Unknown proposal: {}", vote.proposal_id)));
            }
        };

        if !proposal.can_vote() {
            warn!("Received vote for inactive proposal: {}", vote.proposal_id);
            return Err(NetworkError::Validation(format!("Proposal {} is not active", vote.proposal_id)));
        }

        // Check for duplicate vote
        {
            let votes = self.votes.read().await;
            if let Some(proposal_votes) = votes.get(&vote.proposal_id) {
                if proposal_votes.iter().any(|v| v.voter == vote.voter) {
                    warn!("Duplicate vote received from {} for proposal: {}", vote.voter, vote.proposal_id);
                    return Err(NetworkError::Validation(format!("Duplicate vote from {}", vote.voter)));
                }
            }
        }

        // Store vote
        {
            let mut votes = self.votes.write().await;
            let proposal_votes = votes.entry(vote.proposal_id.clone()).or_insert_with(Vec::new);
            proposal_votes.push(vote.clone());
        }

        // Update proposal vote count
        {
            let mut proposals = self.proposals.write().await;
            if let Some(proposal) = proposals.get_mut(&vote.proposal_id) {
                proposal.current_votes += 1;
                
                // Check if quorum is reached
                if proposal.has_quorum() {
                    self.finalize_proposal(&vote.proposal_id).await?;
                }
            }
        }

        info!("Vote processed successfully for proposal: {}", vote.proposal_id);
        Ok(())
    }

    /// Process incoming proposal from another node
    #[instrument(skip(self, proposal), fields(node_id = %self.node_id, proposal_id = %proposal.id, proposer = %proposal.proposer))]
    pub async fn process_proposal(&self, proposal: Proposal) -> Result<()> {
        info!("Processing proposal from {}: {}", proposal.proposer, proposal.id);

        // Extract trace context if present
        // In a real implementation, you would handle trace context propagation here

        // Check if proposal already exists
        {
            let proposals = self.proposals.read().await;
            if proposals.contains_key(&proposal.id) {
                debug!("Proposal {} already exists, ignoring", proposal.id);
                return Ok(());
            }
        }

        // Store proposal
        {
            let mut proposals = self.proposals.write().await;
            proposals.insert(proposal.id.clone(), proposal.clone());
        }

        info!("Proposal {} stored successfully", proposal.id);
        Ok(())
    }

    /// Finalize a proposal when quorum is reached
    #[instrument(skip(self), fields(node_id = %self.node_id, proposal_id))]
    async fn finalize_proposal(&self, proposal_id: &str) -> Result<()> {
        info!("Finalizing proposal: {}", proposal_id);

        let (proposal, votes) = {
            let proposals = self.proposals.read().await;
            let votes = self.votes.read().await;
            
            let proposal = proposals.get(proposal_id).cloned()
                .ok_or_else(|| NetworkError::Validation(format!("Proposal {} not found", proposal_id)))?;
            
            let proposal_votes = votes.get(proposal_id).cloned().unwrap_or_default();
            
            (proposal, proposal_votes)
        };

        // Count votes
        let mut approve_count = 0;
        let mut reject_count = 0;
        let mut abstain_count = 0;

        for vote in &votes {
            match vote.vote_type {
                VoteType::Approve => approve_count += 1,
                VoteType::Reject => reject_count += 1,
                VoteType::Abstain => abstain_count += 1,
            }
        }

        // Determine outcome
        let final_status = if approve_count > reject_count {
            ProposalStatus::Approved
        } else {
            ProposalStatus::Rejected
        };

        // Update proposal status
        {
            let mut proposals = self.proposals.write().await;
            if let Some(proposal) = proposals.get_mut(proposal_id) {
                proposal.status = final_status.clone();
            }
        }

        info!(
            "Proposal {} finalized with status: {:?} (A:{}, R:{}, Abstain:{})",
            proposal_id, final_status, approve_count, reject_count, abstain_count
        );

        Ok(())
    }

    /// Clean up expired proposals
    #[instrument(skip(self), fields(node_id = %self.node_id))]
    pub async fn cleanup_expired_proposals(&self) -> Result<usize> {
        let now = chrono::Utc::now();
        let mut expired_proposals = Vec::new();

        // Find expired proposals
        {
            let proposals = self.proposals.read().await;
            for (id, proposal) in proposals.iter() {
                if proposal.is_expired() && matches!(proposal.status, ProposalStatus::Active) {
                    expired_proposals.push(id.clone());
                }
            }
        }

        // Mark as expired
        if !expired_proposals.is_empty() {
            let mut proposals = self.proposals.write().await;
            for id in &expired_proposals {
                if let Some(proposal) = proposals.get_mut(id) {
                    proposal.status = ProposalStatus::Expired;
                    info!("Proposal {} marked as expired", id);
                }
            }
        }

        if !expired_proposals.is_empty() {
            info!("Cleaned up {} expired proposals", expired_proposals.len());
        }

        Ok(expired_proposals.len())
    }

    /// Get proposal information
    pub async fn get_proposal(&self, proposal_id: &str) -> Option<Proposal> {
        let proposals = self.proposals.read().await;
        proposals.get(proposal_id).cloned()
    }

    /// Get votes for a proposal
    pub async fn get_votes(&self, proposal_id: &str) -> Vec<Vote> {
        let votes = self.votes.read().await;
        votes.get(proposal_id).cloned().unwrap_or_default()
    }

    /// Get all active proposals
    pub async fn get_active_proposals(&self) -> Vec<Proposal> {
        let proposals = self.proposals.read().await;
        proposals
            .values()
            .filter(|p| p.can_vote())
            .cloned()
            .collect()
    }

    /// Get consensus engine statistics
    pub async fn get_stats(&self) -> ConsensusStats {
        let proposals = self.proposals.read().await;
        let votes = self.votes.read().await;

        let total_proposals = proposals.len();
        let active_proposals = proposals.values().filter(|p| p.can_vote()).count();
        let approved_proposals = proposals.values().filter(|p| matches!(p.status, ProposalStatus::Approved)).count();
        let rejected_proposals = proposals.values().filter(|p| matches!(p.status, ProposalStatus::Rejected)).count();
        let expired_proposals = proposals.values().filter(|p| matches!(p.status, ProposalStatus::Expired)).count();

        let total_votes: usize = votes.values().map(|v| v.len()).sum();

        ConsensusStats {
            node_id: self.node_id.clone(),
            total_proposals,
            active_proposals,
            approved_proposals,
            rejected_proposals,
            expired_proposals,
            total_votes,
            required_votes: self.required_votes,
        }
    }

    /// Start background maintenance task
    pub async fn start_maintenance(&self) -> tokio::task::JoinHandle<()> {
        let proposals = self.proposals.clone();
        let node_id = self.node_id.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                let span = tracing::info_span!(
                    "consensus_maintenance",
                    node_id = %node_id,
                    timestamp = %chrono::Utc::now()
                );
                let _enter = span.enter();
                
                // Clean up expired proposals
                let expired_count = {
                    let proposals = proposals.read().await;
                    let mut expired = Vec::new();
                    
                    for (id, proposal) in proposals.iter() {
                        if proposal.is_expired() && matches!(proposal.status, ProposalStatus::Active) {
                            expired.push(id.clone());
                        }
                    }
                    
                    if !expired.is_empty() {
                        let mut proposals = proposals.write().await;
                        for id in &expired {
                            if let Some(proposal) = proposals.get_mut(id) {
                                proposal.status = ProposalStatus::Expired;
                                info!("Proposal {} expired during maintenance", id);
                            }
                        }
                    }
                    
                    expired.len()
                };
                
                if expired_count > 0 {
                    info!("Maintenance: {} proposals expired", expired_count);
                }
            }
        })
    }
}

/// Consensus engine statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusStats {
    pub node_id: String,
    pub total_proposals: usize,
    pub active_proposals: usize,
    pub approved_proposals: usize,
    pub rejected_proposals: usize,
    pub expired_proposals: usize,
    pub total_votes: usize,
    pub required_votes: usize,
}
