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
    use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
    use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};
    use soroban_sdk::{symbol_short, IntoVal};

    #[test]
    fn deposit_withdraw_round_trip() {
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

        vault.deposit(&user, &400);
        assert_eq!(vault.balance(&user), 400);
        assert_eq!(vault.total_deposits(), 400);

        vault.withdraw(&user, &150);
        assert_eq!(vault.balance(&user), 250);
        assert_eq!(vault.total_deposits(), 250);

        assert_eq!(TokenClient::new(&e, &deposit_token_id).balance(&user), 750);
        assert_eq!(TokenClient::new(&e, &deposit_token_id).balance(&vault_id), 250);
    }

    #[test]
    fn rewards_are_proportional_and_claimable() {
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

        vault.deposit(&alice, &100);
        vault.deposit(&bob, &300);

        vault.distribute_rewards(&400);

        assert_eq!(vault.pending_rewards(&alice), 100);
        assert_eq!(vault.pending_rewards(&bob), 300);

        assert_eq!(vault.claim_rewards(&alice), 100);
        assert_eq!(vault.claim_rewards(&bob), 300);

        assert_eq!(TokenClient::new(&e, &reward_token_id).balance(&alice), 100);
        assert_eq!(TokenClient::new(&e, &reward_token_id).balance(&bob), 300);
        assert_eq!(TokenClient::new(&e, &reward_token_id).balance(&vault_id), 0);
    }

    #[test]
    fn rejects_invalid_amounts() {
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

        let err = vault.try_deposit(&user, &0).unwrap_err();
        assert!(matches!(err, Ok(VaultError::InvalidAmount)));

        let err = vault.try_withdraw(&user, &0).unwrap_err();
        assert!(matches!(err, Ok(VaultError::InvalidAmount)));

        let err = vault.try_distribute_rewards(&0).unwrap_err();
        assert!(matches!(err, Ok(VaultError::InvalidAmount)));
    }

    #[test]
    fn cannot_withdraw_more_than_balance() {
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

        let err = vault.try_withdraw(&user, &201).unwrap_err();
        assert!(matches!(err, Ok(VaultError::InsufficientBalance)));
    }

    #[test]
    fn distribute_requires_deposits() {
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

        let err = vault.try_distribute_rewards(&100).unwrap_err();
        assert!(matches!(err, Ok(VaultError::NoDeposits)));
    }

    #[test]
    fn initialization_is_one_time() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);
        let err = vault
            .try_initialize(&admin, &deposit_token_id, &reward_token_id)
            .unwrap_err();
        assert!(matches!(err, Ok(VaultError::AlreadyInitialized)));
    }

    #[test]
    fn deposit_and_withdraw_emit_structured_events_with_timestamps() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set_timestamp(1_710_000_000);

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &400);
        assert!(e.events().all().contains((
            vault_id.clone(),
            (symbol_short!("deposit"),).into_val(&e),
            events::DepositEvent {
                user: user.clone(),
                amount: 400,
                new_balance: 400,
                timestamp: 1_710_000_000,
            }
            .into_val(&e),
        )));

        e.ledger().set_timestamp(1_710_000_123);
        vault.withdraw(&user, &150);
        assert!(e.events().all().contains((
            vault_id,
            (symbol_short!("withdraw"),).into_val(&e),
            events::WithdrawEvent {
                user,
                amount: 150,
                new_balance: 250,
                timestamp: 1_710_000_123,
            }
            .into_val(&e),
        )));
    }

    #[test]
    fn reward_distribution_and_claim_emit_structured_events_with_timestamps() {
        let e = Env::default();
        e.mock_all_auths();
        e.ledger().set_timestamp(1_720_000_000);

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);
        deposit_token.mint(&user, &1_000);
        reward_token.mint(&admin, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);
        vault.deposit(&user, &250);

        let next_idx = vault.distribute_rewards(&500);
        assert!(e.events().all().contains((
            vault_id.clone(),
            (symbol_short!("distrib"),).into_val(&e),
            events::DistributeRewardsEvent {
                caller: admin.clone(),
                amount: 500,
                reward_index: next_idx,
                timestamp: 1_720_000_000,
            }
            .into_val(&e),
        )));

        e.ledger().set_timestamp(1_720_000_111);
        let claimed = vault.claim_rewards(&user);
        assert_eq!(claimed, 500);
        assert!(e.events().all().contains((
            vault_id,
            (symbol_short!("claim"),).into_val(&e),
            events::ClaimRewardsEvent {
                user,
                amount: 500,
                timestamp: 1_720_000_111,
            }
            .into_val(&e),
        )));
    }
}
