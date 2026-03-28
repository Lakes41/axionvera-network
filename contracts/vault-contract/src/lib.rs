#![no_std]

mod errors;
mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, Address, Env};

use crate::errors::VaultError;

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
        validate_init_config(&e, &admin, &deposit_token, &reward_token)?;

        storage::initialize_state(&e, &admin, &deposit_token, &reward_token);

        events::emit_initialize(&e, admin, deposit_token, reward_token);
        Ok(())
    }

    pub fn deposit(e: Env, from: Address, amount: i128) -> Result<(), VaultError> {
        storage::require_initialized(&e)?;
        validate_positive_amount(amount)?;
        from.require_auth();
        with_non_reentrant(&e, || {
            let state = storage::get_state(&e)?;
            let token_id = state.deposit_token.clone();
            let token = soroban_sdk::token::Client::new(&e, &token_id);
            token.transfer(&from, &e.current_contract_address(), &amount);

            let (_, position) = storage::store_deposit(&e, &from, amount)?;
            events::emit_deposit(&e, from, amount, position.balance);
            Ok(())
        })
    }

    pub fn withdraw(e: Env, to: Address, amount: i128) -> Result<(), VaultError> {
        storage::require_initialized(&e)?;
        validate_positive_amount(amount)?;
        to.require_auth();
        with_non_reentrant(&e, || {
            let state = storage::get_state(&e)?;
            let token_id = state.deposit_token.clone();
            let token = soroban_sdk::token::Client::new(&e, &token_id);

            let (_, position) = storage::store_withdraw(&e, &to, amount)?;
            let next_balance = position.balance;
            token.transfer(&e.current_contract_address(), &to, &amount);

            events::emit_withdraw(&e, to, amount, next_balance);
            Ok(())
        })
    }

    pub fn distribute_rewards(e: Env, amount: i128) -> Result<i128, VaultError> {
        storage::require_initialized(&e)?;
        validate_positive_amount(amount)?;

        let state = storage::get_state(&e)?;
        let admin = state.admin.clone();
        admin.require_auth();
        with_non_reentrant(&e, || {
            let reward_token_id = state.reward_token.clone();
            let reward_token = soroban_sdk::token::Client::new(&e, &reward_token_id);
            reward_token.transfer(&admin, &e.current_contract_address(), &amount);

            let next_state = storage::store_reward_distribution(&e, amount)?;
            let next_idx = next_state.reward_index;
            events::emit_distribute(&e, admin, amount, next_idx);
            Ok(next_idx)
        })
    }

    pub fn claim_rewards(e: Env, user: Address) -> Result<i128, VaultError> {
        storage::require_initialized(&e)?;
        user.require_auth();
        with_non_reentrant(&e, || {
            let state = storage::get_state(&e)?;
            let amt = storage::store_claimable_rewards(&e, &user)?;
            if amt <= 0 {
                return Ok(0);
            }

            let reward_token_id = state.reward_token.clone();
            let reward_token = soroban_sdk::token::Client::new(&e, &reward_token_id);
            reward_token.transfer(&e.current_contract_address(), &user, &amt);

            events::emit_claim(&e, user, amt);
            Ok(amt)
        })
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

fn validate_positive_amount(amount: i128) -> Result<(), VaultError> {
    if amount <= 0 {
        return Err(VaultError::InvalidAmount);
    }
    Ok(())
}

fn validate_init_config(
    e: &Env,
    admin: &Address,
    deposit_token: &Address,
    reward_token: &Address,
) -> Result<(), VaultError> {
    let contract = e.current_contract_address();
    if admin == &contract || deposit_token == &contract || reward_token == &contract {
        return Err(VaultError::InvalidConfiguration);
    }

    Ok(())
}

fn with_non_reentrant<T, F>(e: &Env, f: F) -> Result<T, VaultError>
where
    F: FnOnce() -> Result<T, VaultError>,
{
    storage::enter_non_reentrant(e)?;
    let result = f();
    storage::exit_non_reentrant(e);
    result
}

// TODO(reward-optimization): Consider a higher precision / rounding strategy for small totals.
// TODO(security): Consider adding pausability or per-user deposit caps.
// TODO(governance): Introduce admin handover / multisig patterns.
// TODO(upgradeability): Evaluate upgrade patterns compatible with Soroban best practices.

#[cfg(test)]
mod test {
    use super::*;
    use crate::storage::REWARD_INDEX_SCALE;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

    #[test]
    fn deposit_withdraw_round_trip() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token_admin = StellarAssetClient::new(&e, &deposit_token_id);
        let deposit_token = TokenClient::new(&e, &deposit_token_id);
        deposit_token_admin.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &400);
        assert_eq!(vault.balance(&user), 400);
        assert_eq!(vault.total_deposits(), 400);

        vault.withdraw(&user, &150);
        assert_eq!(vault.balance(&user), 250);
        assert_eq!(vault.total_deposits(), 250);

        assert_eq!(deposit_token.balance(&user), 750);
        assert_eq!(deposit_token.balance(&vault_id), 250);
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

        let deposit_token_admin = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token_admin = StellarAssetClient::new(&e, &reward_token_id);
        let reward_token = TokenClient::new(&e, &reward_token_id);

        deposit_token_admin.mint(&alice, &1_000);
        deposit_token_admin.mint(&bob, &1_000);
        reward_token_admin.mint(&admin, &1_000);

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

        assert_eq!(reward_token.balance(&alice), 100);
        assert_eq!(reward_token.balance(&bob), 300);
        assert_eq!(reward_token.balance(&vault_id), 0);
    }

    #[test]
    fn rejects_invalid_amounts() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token_admin = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token_admin.mint(&user, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let err = vault.try_deposit(&user, &0).unwrap_err();
        assert_eq!(err, Ok(VaultError::InvalidAmount));

        let err = vault.try_withdraw(&user, &0).unwrap_err();
        assert_eq!(err, Ok(VaultError::InvalidAmount));

        let err = vault.try_distribute_rewards(&0).unwrap_err();
        assert_eq!(err, Ok(VaultError::InvalidAmount));
    }

    #[test]
    fn cannot_withdraw_more_than_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token_admin = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token_admin.mint(&user, &500);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &200);

        let err = vault.try_withdraw(&user, &201).unwrap_err();
        assert_eq!(err, Ok(VaultError::InsufficientBalance));
    }

    #[test]
    fn distribute_requires_deposits() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let reward_token_admin = StellarAssetClient::new(&e, &reward_token_id);
        reward_token_admin.mint(&admin, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let err = vault.try_distribute_rewards(&100).unwrap_err();
        assert_eq!(err, Ok(VaultError::NoDeposits));
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
        assert_eq!(err, Ok(VaultError::AlreadyInitialized));
    }

    #[test]
    fn rejects_invalid_initialization_configuration() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);

        let err = vault.try_initialize(&admin, &vault_id, &admin).unwrap_err();
        assert_eq!(err, Ok(VaultError::InvalidConfiguration));
    }

    #[test]
    fn distribution_rejects_zero_index_increment() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e.register_stellar_asset_contract(admin.clone());
        let reward_token_id = e.register_stellar_asset_contract(admin.clone());

        let deposit_token_admin = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token_admin = StellarAssetClient::new(&e, &reward_token_id);
        let oversized_total = REWARD_INDEX_SCALE + 1;
        deposit_token_admin.mint(&user, &oversized_total);
        reward_token_admin.mint(&admin, &1_000);

        let vault_id = e.register_contract(None, VaultContract);
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);
        vault.deposit(&user, &oversized_total);

        let err = vault.try_distribute_rewards(&1).unwrap_err();
        assert_eq!(err, Ok(VaultError::ZeroRewardIncrement));
    }
}
