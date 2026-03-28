#![no_std]

mod errors;
mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, Address, Env};

use crate::errors::VaultError;
use crate::storage::REWARD_INDEX_SCALE;

#[contract]
pub struct VaultContract;

#[contractimpl]
impl VaultContract {
    pub fn version() -> u32 {
        1
    }

    pub fn initialize(
        e: Env,
        admin: Address,
        deposit_token: Address,
        reward_token: Address,
    ) -> Result<(), VaultError> {
        if storage::is_initialized(&e) {
            return Err(VaultError::AlreadyInitialized);
        }

        admin.require_auth();

        storage::set_initialized(&e);
        storage::set_admin(&e, &admin);
        storage::set_deposit_token(&e, &deposit_token);
        storage::set_reward_token(&e, &reward_token);
        storage::set_total_deposits(&e, 0);
        storage::set_reward_index(&e, 0);

        events::emit_initialize(&e, admin, deposit_token, reward_token);
        Ok(())
    }

    pub fn deposit(e: Env, from: Address, amount: i128) -> Result<(), VaultError> {
        storage::require_initialized(&e)?;
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }

        from.require_auth();

        storage::accrue_user_rewards(&e, &from)?;

        let token_id = storage::get_deposit_token(&e)?;
        let token = soroban_sdk::token::Client::new(&e, &token_id);
        token.transfer(&from, &e.current_contract_address(), &amount);

        let prev_balance = storage::get_user_balance(&e, &from)?;
        let next_balance = prev_balance
            .checked_add(amount)
            .ok_or(VaultError::MathOverflow)?;
        storage::set_user_balance(&e, &from, next_balance);

        let prev_total = storage::get_total_deposits(&e)?;
        let next_total = prev_total
            .checked_add(amount)
            .ok_or(VaultError::MathOverflow)?;
        storage::set_total_deposits(&e, next_total);

        events::emit_deposit(&e, from, amount, next_balance);
        Ok(())
    }

    pub fn withdraw(e: Env, to: Address, amount: i128) -> Result<(), VaultError> {
        storage::require_initialized(&e)?;
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }

        to.require_auth();

        storage::accrue_user_rewards(&e, &to)?;

        let prev_balance = storage::get_user_balance(&e, &to)?;
        if prev_balance < amount {
            return Err(VaultError::InsufficientBalance);
        }
        let next_balance = prev_balance
            .checked_sub(amount)
            .ok_or(VaultError::MathOverflow)?;
        storage::set_user_balance(&e, &to, next_balance);

        let prev_total = storage::get_total_deposits(&e)?;
        let next_total = prev_total
            .checked_sub(amount)
            .ok_or(VaultError::MathOverflow)?;
        storage::set_total_deposits(&e, next_total);

        let token_id = storage::get_deposit_token(&e)?;
        let token = soroban_sdk::token::Client::new(&e, &token_id);
        token.transfer(&e.current_contract_address(), &to, &amount);

        events::emit_withdraw(&e, to, amount, next_balance);
        Ok(())
    }

    pub fn distribute_rewards(e: Env, amount: i128) -> Result<i128, VaultError> {
        storage::require_initialized(&e)?;
        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }

        let admin = storage::get_admin(&e)?;
        admin.require_auth();

        let total = storage::get_total_deposits(&e)?;
        if total <= 0 {
            return Err(VaultError::NoDeposits);
        }

        let reward_token_id = storage::get_reward_token(&e)?;
        let reward_token = soroban_sdk::token::Client::new(&e, &reward_token_id);
        reward_token.transfer(&admin, &e.current_contract_address(), &amount);

        let increment = amount
            .checked_mul(REWARD_INDEX_SCALE)
            .ok_or(VaultError::MathOverflow)?
            / total;

        let prev_idx = storage::get_reward_index(&e)?;
        let next_idx = prev_idx
            .checked_add(increment)
            .ok_or(VaultError::MathOverflow)?;
        storage::set_reward_index(&e, next_idx);

        events::emit_distribute(&e, admin, amount, next_idx);
        Ok(next_idx)
    }

    pub fn claim_rewards(e: Env, user: Address) -> Result<i128, VaultError> {
        storage::require_initialized(&e)?;
        user.require_auth();

        storage::accrue_user_rewards(&e, &user)?;
        let amt = storage::get_user_rewards(&e, &user)?;
        if amt <= 0 {
            return Ok(0);
        }

        storage::set_user_rewards(&e, &user, 0);

        let reward_token_id = storage::get_reward_token(&e)?;
        let reward_token = soroban_sdk::token::Client::new(&e, &reward_token_id);
        reward_token.transfer(&e.current_contract_address(), &user, &amt);

        events::emit_claim(&e, user, amt);
        Ok(amt)
    }

    pub fn balance(e: Env, user: Address) -> Result<i128, VaultError> {
        storage::get_user_balance(&e, &user)
    }

    pub fn total_deposits(e: Env) -> Result<i128, VaultError> {
        storage::get_total_deposits(&e)
    }

    pub fn reward_index(e: Env) -> Result<i128, VaultError> {
        storage::get_reward_index(&e)
    }

    pub fn pending_rewards(e: Env, user: Address) -> Result<i128, VaultError> {
        storage::pending_user_rewards_view(&e, &user)
    }

    pub fn admin(e: Env) -> Result<Address, VaultError> {
        storage::get_admin(&e)
    }

    pub fn deposit_token(e: Env) -> Result<Address, VaultError> {
        storage::get_deposit_token(&e)
    }

    pub fn reward_token(e: Env) -> Result<Address, VaultError> {
        storage::get_reward_token(&e)
    }
}

// TODO(reward-optimization): Consider a higher precision / rounding strategy for small totals.
// TODO(gas): Consider merging per-user keys (balance/index/rewards) into a single struct to reduce reads.
// TODO(security): Consider adding pausability or per-user deposit caps.
// TODO(governance): Introduce admin handover / multisig patterns.
// TODO(upgradeability): Evaluate upgrade patterns compatible with Soroban best practices.

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    // ===== Deposit Logic Tests =====

    #[test]
    fn test_deposit_logic() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Test single deposit
        vault.deposit(&user, &400);
        assert_eq!(vault.balance(&user), 400);
        assert_eq!(vault.total_deposits(), 400);

        // Test multiple deposits accumulate
        vault.deposit(&user, &200);
        assert_eq!(vault.balance(&user), 600);
        assert_eq!(vault.total_deposits(), 600);
    }

    #[test]
    fn test_multiple_deposits_accumulate() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &5_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &100);
        assert_eq!(vault.balance(&user), 100);
        assert_eq!(vault.total_deposits(), 100);

        vault.deposit(&user, &200);
        assert_eq!(vault.balance(&user), 300);
        assert_eq!(vault.total_deposits(), 300);

        vault.deposit(&user, &700);
        assert_eq!(vault.balance(&user), 1_000);
        assert_eq!(vault.total_deposits(), 1_000);
    }

    #[test]
    fn test_deposit_increases_total() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user1 = Address::generate(&e);
        let user2 = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user1, &1_000);
        deposit_token.mint(&user2, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let amount1 = 100_i128;
        let amount2 = 250_i128;

        vault.deposit(&user1, &amount1);
        assert_eq!(vault.total_deposits(), amount1);

        vault.deposit(&user2, &amount2);
        assert_eq!(vault.total_deposits(), amount1 + amount2);
    }

    // ===== Withdraw Logic Tests =====

    #[test]
    fn test_withdraw_logic() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Test withdraw logic
        vault.deposit(&user, &500);
        assert_eq!(vault.balance(&user), 500);

        vault.withdraw(&user, &200);
        assert_eq!(vault.balance(&user), 300);
        assert_eq!(vault.total_deposits(), 300);
    }

    #[test]
    fn test_multiple_withdrawals_work_correctly() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &1_000);
        assert_eq!(vault.balance(&user), 1_000);

        vault.withdraw(&user, &100);
        assert_eq!(vault.balance(&user), 900);
        assert_eq!(vault.total_deposits(), 900);

        vault.withdraw(&user, &250);
        assert_eq!(vault.balance(&user), 650);
        assert_eq!(vault.total_deposits(), 650);

        vault.withdraw(&user, &650);
        assert_eq!(vault.balance(&user), 0);
        assert_eq!(vault.total_deposits(), 0);
    }

    #[test]
    fn test_withdraw_entire_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let amount = 500_i128;
        vault.deposit(&user, &amount);
        vault.withdraw(&user, &amount);

        assert_eq!(vault.balance(&user), 0);
        assert_eq!(vault.total_deposits(), 0);
    }

    #[test]
    fn test_deposit_after_reward_distribution() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user1 = Address::generate(&e);
        let user2 = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&user1, &1_000);
        deposit_token.mint(&user2, &1_000);
        reward_token.mint(&admin, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let deposit1 = 500_i128;
        let reward_amount = 100_i128;
        let deposit2 = 200_i128;

        vault.deposit(&user1, &deposit1);
        vault.distribute_rewards(&reward_amount);
        vault.deposit(&user2, &deposit2);

        let total = vault.total_deposits();
        assert_eq!(total, deposit1 + deposit2);
    }

    // ===== Reward Distribution Tests =====

    #[test]
    fn test_reward_distribution() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&user, &1_000);
        reward_token.mint(&admin, &100);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let deposit = 1_000_i128;
        let reward = 100_i128;

        vault.deposit(&user, &deposit);

        let reward_index_before = vault.reward_index();
        assert_eq!(reward_index_before, 0);

        vault.distribute_rewards(&reward);

        let reward_index_after = vault.reward_index();
        assert!(reward_index_after > 0);
    }

    #[test]
    fn test_rewards_are_proportional_and_claimable() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let alice = Address::generate(&e);
        let bob = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&alice, &1_000);
        deposit_token.mint(&bob, &1_000);
        reward_token.mint(&admin, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // User1 deposits 2x more than User2
        let deposit1 = 200_i128;
        let deposit2 = 100_i128;
        let reward_amount = 300_i128;

        vault.deposit(&alice, &deposit1);
        vault.deposit(&bob, &deposit2);

        vault.distribute_rewards(&reward_amount);

        let pending_alice = vault.pending_rewards(&alice);
        let pending_bob = vault.pending_rewards(&bob);

        assert!(pending_alice > 0);
        assert!(pending_bob > 0);
        assert!(pending_alice > pending_bob);

        let claimed_alice = vault.claim_rewards(&alice);
        let claimed_bob = vault.claim_rewards(&bob);

        assert_eq!(claimed_alice, pending_alice);
        assert_eq!(claimed_bob, pending_bob);
    }

    #[test]
    fn test_multiple_reward_distributions_accumulate() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&user, &1_000);
        reward_token.mint(&admin, &3_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let deposit = 1_000_i128;
        let reward1 = 100_i128;
        let reward2 = 50_i128;

        vault.deposit(&user, &deposit);
        vault.distribute_rewards(&reward1);

        let pending_after_first = vault.pending_rewards(&user);

        vault.distribute_rewards(&reward2);

        let pending_after_second = vault.pending_rewards(&user);

        assert!(pending_after_second > pending_after_first);
    }

    #[test]
    fn test_reward_proportionality_with_unequal_deposits() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user1 = Address::generate(&e);
        let user2 = Address::generate(&e);
        let user3 = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&user1, &1_000);
        deposit_token.mint(&user2, &2_000);
        deposit_token.mint(&user3, &3_000);
        reward_token.mint(&admin, &6_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Deposits in ratio 1:2:3
        vault.deposit(&user1, &1_000);
        vault.deposit(&user2, &2_000);
        vault.deposit(&user3, &3_000);

        // Distribute 600 rewards
        vault.distribute_rewards(&600);

        let pending1 = vault.pending_rewards(&user1);
        let pending2 = vault.pending_rewards(&user2);
        let pending3 = vault.pending_rewards(&user3);

        // Rewards should be proportional to deposits
        assert_eq!(pending1, 100);
        assert_eq!(pending2, 200);
        assert_eq!(pending3, 300);

        let claimed1 = vault.claim_rewards(&user1);
        let claimed2 = vault.claim_rewards(&user2);
        let claimed3 = vault.claim_rewards(&user3);

        assert_eq!(claimed1, 100);
        assert_eq!(claimed2, 200);
        assert_eq!(claimed3, 300);
    }

    #[test]
    fn test_claim_with_zero_rewards() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Try to claim without any rewards
        let claimed = vault.claim_rewards(&user);
        assert_eq!(claimed, 0);
    }

    // ===== Edge Cases Tests =====

    #[test]
    fn test_rejects_invalid_amounts() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Test zero deposit
        assert!(vault.try_deposit(&user, &0).is_err());

        // Test negative deposit
        assert!(vault.try_deposit(&user, &-1_000).is_err());

        // Test zero withdraw
        assert!(vault.try_withdraw(&user, &0).is_err());

        // Test negative withdraw
        assert!(vault.try_withdraw(&user, &-500).is_err());
    }

    #[test]
    fn test_negative_deposits_rejected() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        assert!(vault.try_deposit(&user, &-100).is_err());
    }

    #[test]
    fn test_negative_withdrawals_rejected() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &500);

        assert!(vault.try_withdraw(&user, &-100).is_err());
    }

    #[test]
    fn test_cannot_withdraw_more_than_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &500);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &200);

        assert!(vault.try_withdraw(&user, &201).is_err());
    }

    #[test]
    fn test_large_deposit_and_withdraw() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let large_amount = 9_223_372_036_854_775_000i128; // Near i128 max
        deposit_token.mint(&user, &large_amount);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &large_amount);
        assert_eq!(vault.balance(&user), large_amount);
        assert_eq!(vault.total_deposits(), large_amount);

        vault.withdraw(&user, &large_amount);
        assert_eq!(vault.balance(&user), 0);
        assert_eq!(vault.total_deposits(), 0);
    }

    #[test]
    fn test_distribute_requires_deposits() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let reward_token = StellarAssetClient::new(&e, &reward_token_id);
        reward_token.mint(&admin, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Try to distribute rewards without any deposits
        assert!(vault.try_distribute_rewards(&100).is_err());
    }

    #[test]
    fn test_initialization_is_one_time() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);
        assert!(vault
            .try_initialize(&admin, &deposit_token_id, &reward_token_id)
            .is_err());
    }

    // ===== Query Tests =====

    #[test]
    fn test_query_functions_return_correct_values() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Test version
        assert_eq!(VaultContract::version(), 1);

        // Test initial state
        assert_eq!(vault.total_deposits(), 0);
        assert_eq!(vault.reward_index(), 0);
        assert_eq!(vault.balance(&user), 0);
        assert_eq!(vault.pending_rewards(&user), 0);
    }

    // ===== Round-Trip Tests =====

    #[test]
    fn test_deposit_withdraw_round_trip() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let deposit = 750_i128;
        let withdraw = 500_i128;

        vault.deposit(&user, &deposit);
        let balance_after_deposit = vault.balance(&user);
        assert_eq!(balance_after_deposit, deposit);

        vault.withdraw(&user, &withdraw);
        let balance_after_withdraw = vault.balance(&user);
        assert_eq!(balance_after_withdraw, deposit - withdraw);

        let total = vault.total_deposits();
        assert_eq!(total, deposit - withdraw);
    }

    #[test]
    fn test_multiple_users_deposits_and_withdrawals() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user1 = Address::generate(&e);
        let user2 = Address::generate(&e);
        let user3 = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user1, &2_000);
        deposit_token.mint(&user2, &3_000);
        deposit_token.mint(&user3, &1_500);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let deposit1 = 800_i128;
        let deposit2 = 1_200_i128;
        let deposit3 = 500_i128;

        vault.deposit(&user1, &deposit1);
        assert_eq!(vault.balance(&user1), deposit1);
        assert_eq!(vault.total_deposits(), deposit1);

        vault.deposit(&user2, &deposit2);
        assert_eq!(vault.balance(&user2), deposit2);
        assert_eq!(vault.total_deposits(), deposit1 + deposit2);

        vault.deposit(&user3, &deposit3);
        assert_eq!(vault.balance(&user3), deposit3);
        assert_eq!(vault.total_deposits(), deposit1 + deposit2 + deposit3);

        vault.withdraw(&user1, &300);
        assert_eq!(vault.balance(&user1), deposit1 - 300);
        assert_eq!(vault.total_deposits(), deposit1 - 300 + deposit2 + deposit3);

        vault.withdraw(&user2, &400);
        assert_eq!(vault.balance(&user2), deposit2 - 400);

        let final_total =
            (deposit1 - 300) + (deposit2 - 400) + deposit3;
        assert_eq!(vault.total_deposits(), final_total);
    }

    #[test]
    fn test_reward_accrual_on_deposit_withdrawal_sequence() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&user, &5_000);
        reward_token.mint(&admin, &2_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let deposit = 1_000_i128;
        let reward = 200_i128;

        vault.deposit(&user, &deposit);
        vault.distribute_rewards(&reward);

        let pending_before_withdraw = vault.pending_rewards(&user);
        assert!(pending_before_withdraw > 0);

        vault.withdraw(&user, &(deposit / 2));

        let pending_after_withdraw = vault.pending_rewards(&user);
        assert_eq!(pending_before_withdraw, pending_after_withdraw);
    }

    #[test]
    fn test_edge_cases() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Test zero values are rejected
        assert!(vault.try_deposit(&user, &0).is_err());
        assert!(vault.try_deposit(&user, &-50).is_err());

        // Test insufficient balance on withdraw
        vault.deposit(&user, &200);
        assert!(vault.try_withdraw(&user, &201).is_err());

        // Test invalid amounts
        assert!(vault.try_withdraw(&user, &0).is_err());
    }
}
