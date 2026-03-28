//! On-chain-style network parameters from genesis, scheduled upgrades, and activation epochs.
//!
//! Upgrades are **announced** when the `ParameterUpgrade` transaction is accepted and recorded
//! against the current chain tip. They **apply** at `activation_epoch_height` so every honest
//! node that processes the same blocks transitions at the same height.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Root genesis document loaded from JSON (see `config/genesis.example.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisDocument {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub chain_id: String,
    #[serde(default)]
    pub genesis_time_rfc3339: Option<String>,
    pub network_parameters: NetworkParameters,
    pub parameter_upgrade_governance: GovernanceConfig,
    #[serde(default = "default_min_activation_delay_blocks")]
    pub min_activation_delay_blocks: u64,
}

fn default_schema_version() -> u32 {
    1
}

fn default_min_activation_delay_blocks() -> u64 {
    100
}

/// Live tunable network limits (consensus-relevant in a full implementation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkParameters {
    pub max_block_body_bytes: u64,
    pub min_base_fee: u64,
    pub max_transactions_per_block: u32,
}

/// Partial update merged at activation (unset fields keep the previous value).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkParametersPatch {
    pub max_block_body_bytes: Option<u64>,
    pub min_base_fee: Option<u64>,
    pub max_transactions_per_block: Option<u32>,
}

impl NetworkParameters {
    pub fn apply_patch(&self, patch: &NetworkParametersPatch) -> NetworkParameters {
        NetworkParameters {
            max_block_body_bytes: patch
                .max_block_body_bytes
                .unwrap_or(self.max_block_body_bytes),
            min_base_fee: patch.min_base_fee.unwrap_or(self.min_base_fee),
            max_transactions_per_block: patch
                .max_transactions_per_block
                .unwrap_or(self.max_transactions_per_block),
        }
    }
}

fn merge_patches(into: &mut NetworkParametersPatch, add: &NetworkParametersPatch) {
    if add.max_block_body_bytes.is_some() {
        into.max_block_body_bytes = add.max_block_body_bytes;
    }
    if add.min_base_fee.is_some() {
        into.min_base_fee = add.min_base_fee;
    }
    if add.max_transactions_per_block.is_some() {
        into.max_transactions_per_block = add.max_transactions_per_block;
    }
}

/// Who may submit a `ParameterUpgrade` transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum GovernanceConfig {
    /// One of the listed keys must sign (see `authorize_upgrade`); addresses are compared case-insensitively.
    AdminKeys { keys: Vec<String> },
    /// A set of distinct DAO members must endorse the upgrade; each id must appear in `members`.
    Dao {
        members: Vec<String>,
        min_approvals: u32,
    },
}

/// Recorded upgrade for APIs and audit (may still be in the future relative to tip).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledUpgradeRecord {
    pub transaction_id: String,
    pub announced_at_height: u64,
    pub activation_epoch_height: u64,
    pub patch: NetworkParametersPatch,
}

/// In-memory registry: genesis base + scheduled activations by block height.
#[derive(Debug, Clone)]
pub struct ChainParameterRegistry {
    chain_id: String,
    governance: GovernanceConfig,
    min_activation_delay_blocks: u64,
    /// Last finalized / logical chain tip used for upgrade validation and status.
    current_height: u64,
    genesis_parameters: NetworkParameters,
    /// Patches keyed by activation height (merged if several txs target the same height).
    activations: BTreeMap<u64, NetworkParametersPatch>,
    /// All submitted upgrades (including already activated) for history APIs.
    upgrade_history: Vec<ScheduledUpgradeRecord>,
}

impl ChainParameterRegistry {
    pub fn from_genesis_file(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read genesis file {}: {}", path.display(), e))?;
        let doc: GenesisDocument = serde_json::from_str(&raw)
            .map_err(|e| format!("Invalid genesis JSON: {}", e))?;
        Ok(Self::from_genesis_document(doc))
    }

    pub fn from_genesis_document(doc: GenesisDocument) -> Self {
        Self {
            chain_id: doc.chain_id,
            governance: doc.parameter_upgrade_governance,
            min_activation_delay_blocks: doc.min_activation_delay_blocks,
            current_height: 0,
            genesis_parameters: doc.network_parameters.clone(),
            activations: BTreeMap::new(),
            upgrade_history: Vec::new(),
        }
    }

    /// Default used when no `GENESIS_CONFIG_PATH` is set (local dev only).
    pub fn development_default() -> Self {
        let doc = GenesisDocument {
            schema_version: 1,
            chain_id: "axionvera-dev".to_string(),
            genesis_time_rfc3339: None,
            network_parameters: NetworkParameters {
                max_block_body_bytes: 2_097_152,
                min_base_fee: 100,
                max_transactions_per_block: 1000,
            },
            parameter_upgrade_governance: GovernanceConfig::AdminKeys {
                keys: vec!["dev-admin".to_string()],
            },
            min_activation_delay_blocks: 10,
        };
        Self::from_genesis_document(doc)
    }

    pub fn chain_id(&self) -> &str {
        &self.chain_id
    }

    pub fn current_height(&self) -> u64 {
        self.current_height
    }

    pub fn min_activation_delay_blocks(&self) -> u64 {
        self.min_activation_delay_blocks
    }

    pub fn genesis_parameters(&self) -> &NetworkParameters {
        &self.genesis_parameters
    }

    /// Effective parameters at `height` (genesis + all activations with key ≤ height).
    pub fn effective_parameters_at(&self, height: u64) -> NetworkParameters {
        let mut p = self.genesis_parameters.clone();
        for (&h, patch) in self.activations.iter() {
            if h <= height {
                p = p.apply_patch(patch);
            }
        }
        p
    }

    /// Parameters currently enforced at the chain tip.
    pub fn active_parameters(&self) -> NetworkParameters {
        self.effective_parameters_at(self.current_height)
    }

    /// Upgrades whose activation height is strictly greater than the current tip (announced, not yet live).
    pub fn pending_upgrades(&self) -> Vec<ScheduledUpgradeRecord> {
        self.upgrade_history
            .iter()
            .filter(|r| r.activation_epoch_height > self.current_height)
            .cloned()
            .collect()
    }

    /// Advance the logical chain tip (hook for consensus / sync). Applying blocks should call this.
    pub fn set_chain_tip_height(&mut self, height: u64) {
        self.current_height = height;
    }

    /// Submit a parameter upgrade: validates governance, delay, and non-empty patch; schedules activation.
    pub fn submit_parameter_upgrade(
        &mut self,
        patch: NetworkParametersPatch,
        activation_epoch_height: u64,
        proposer_address: &str,
        dao_voter_addresses: &[String],
    ) -> Result<String, String> {
        if !patch_has_changes(&patch) {
            return Err("parameter patch must set at least one field".to_string());
        }

        self.authorize_upgrade(proposer_address, dao_voter_addresses)?;

        let tip = self.current_height;
        let min_h = tip.saturating_add(self.min_activation_delay_blocks);
        if activation_epoch_height < min_h {
            return Err(format!(
                "activation_epoch_height {} must be >= {} (tip {} + min_delay {})",
                activation_epoch_height, min_h, tip, self.min_activation_delay_blocks
            ));
        }

        self.activations
            .entry(activation_epoch_height)
            .and_modify(|existing| merge_patches(existing, &patch))
            .or_insert_with(|| patch.clone());

        let tx_id = format!("0x{:064x}", uuid::Uuid::new_v4().as_u128());
        self.upgrade_history.push(ScheduledUpgradeRecord {
            transaction_id: tx_id.clone(),
            announced_at_height: tip,
            activation_epoch_height,
            patch,
        });

        Ok(tx_id)
    }

    fn authorize_upgrade(
        &self,
        proposer_address: &str,
        dao_voter_addresses: &[String],
    ) -> Result<(), String> {
        match &self.governance {
            GovernanceConfig::AdminKeys { keys } => {
                if dao_voter_addresses.iter().any(|v| !v.is_empty()) {
                    return Err("dao_voter_addresses must be empty for admin_keys governance".to_string());
                }
                let p = normalize_id(proposer_address);
                if p.is_empty() {
                    return Err("proposer_address is required for admin_keys governance".to_string());
                }
                let ok = keys.iter().any(|k| normalize_id(k) == p);
                if !ok {
                    return Err("proposer is not an authorized admin key".to_string());
                }
                Ok(())
            }
            GovernanceConfig::Dao {
                members,
                min_approvals,
            } => {
                if *min_approvals == 0 {
                    return Err("dao min_approvals must be > 0".to_string());
                }
                let member_set: HashSet<String> = members.iter().map(|m| normalize_id(m)).collect();
                let mut seen = HashSet::new();
                let mut valid = 0u32;
                for v in dao_voter_addresses {
                    let n = normalize_id(v);
                    if n.is_empty() {
                        continue;
                    }
                    if !member_set.contains(&n) {
                        return Err(format!("unknown dao voter: {}", v));
                    }
                    if seen.insert(n) {
                        valid += 1;
                    }
                }
                if valid < *min_approvals {
                    return Err(format!(
                        "dao consensus requires {} distinct member approvals, got {}",
                        min_approvals, valid
                    ));
                }
                Ok(())
            }
        }
    }
}

fn normalize_id(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn patch_has_changes(p: &NetworkParametersPatch) -> bool {
    p.max_block_body_bytes.is_some()
        || p.min_base_fee.is_some()
        || p.max_transactions_per_block.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry_admin() -> ChainParameterRegistry {
        let doc = GenesisDocument {
            schema_version: 1,
            chain_id: "test".to_string(),
            genesis_time_rfc3339: None,
            network_parameters: NetworkParameters {
                max_block_body_bytes: 1_000_000,
                min_base_fee: 50,
                max_transactions_per_block: 500,
            },
            parameter_upgrade_governance: GovernanceConfig::AdminKeys {
                keys: vec!["Admin_ABC".to_string()],
            },
            min_activation_delay_blocks: 5,
        };
        ChainParameterRegistry::from_genesis_document(doc)
    }

    #[test]
    fn activation_epoch_applies_only_after_height() {
        let mut r = test_registry_admin();
        r.set_chain_tip_height(100);
        let patch = NetworkParametersPatch {
            min_base_fee: Some(99),
            ..Default::default()
        };
        let tx = r
            .submit_parameter_upgrade(patch.clone(), 106, "admin_abc", &[])
            .unwrap();
        assert!(!tx.is_empty());
        assert_eq!(r.active_parameters().min_base_fee, 50);
        assert_eq!(r.effective_parameters_at(105).min_base_fee, 50);
        assert_eq!(r.effective_parameters_at(106).min_base_fee, 99);
        r.set_chain_tip_height(200);
        assert_eq!(r.active_parameters().min_base_fee, 99);
    }

    #[test]
    fn dao_requires_distinct_members() {
        let doc = GenesisDocument {
            schema_version: 1,
            chain_id: "test-dao".to_string(),
            genesis_time_rfc3339: None,
            network_parameters: NetworkParameters {
                max_block_body_bytes: 1,
                min_base_fee: 1,
                max_transactions_per_block: 1,
            },
            parameter_upgrade_governance: GovernanceConfig::Dao {
                members: vec!["m1".to_string(), "m2".to_string(), "m3".to_string()],
                min_approvals: 2,
            },
            min_activation_delay_blocks: 1,
        };
        let mut r = ChainParameterRegistry::from_genesis_document(doc);
        r.set_chain_tip_height(10);
        let patch = NetworkParametersPatch {
            max_block_body_bytes: Some(999),
            ..Default::default()
        };
        assert!(r
            .submit_parameter_upgrade(patch.clone(), 12, "", &["m1".to_string()])
            .is_err());
        r.submit_parameter_upgrade(patch, 12, "", &["m1".to_string(), "m2".to_string()])
            .unwrap();
        assert_eq!(r.pending_upgrades().len(), 1);
    }
}
