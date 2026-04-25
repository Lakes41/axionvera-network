#![no_std]
extern crate alloc;

mod errors;
mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, Address, Env};

use crate::errors::{ArithmeticError, BalanceError, StateError, ValidationError, VaultError};
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
            return Err(StateError::AlreadyInitialized.into());
        }
        validate_distinct_token_addresses(&deposit_token, &reward_token)?;

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
        validate_positive_amount(amount)?;
        from.require_auth();

        let reward_snapshot = storage::preview_user_rewards(&e, &from)?;

        let token_id = storage::get_deposit_token(&e)?;
        let token = soroban_sdk::token::Client::new(&e, &token_id);
        ensure_balance(token.balance(&from), amount)?;

        let prev_balance = storage::get_user_balance(&e, &from)?;
        let next_balance = prev_balance.checked_add(amount).ok_or_else(overflow)?;

        let prev_total = storage::get_total_deposits(&e)?;
        let next_total = prev_total.checked_add(amount).ok_or_else(overflow)?;

        token.transfer(&from, &e.current_contract_address(), &amount);

        storage::apply_user_reward_snapshot(&e, &from, &reward_snapshot);
        storage::set_user_balance(&e, &from, next_balance);
        storage::set_total_deposits(&e, next_total);

        events::emit_deposit(&e, from, amount, next_balance);
        Ok(())
    }

    pub fn withdraw(e: Env, to: Address, amount: i128) -> Result<(), VaultError> {
        storage::require_initialized(&e)?;
        validate_positive_amount(amount)?;
        to.require_auth();

        let reward_snapshot = storage::preview_user_rewards(&e, &to)?;

        let prev_balance = storage::get_user_balance(&e, &to)?;
        ensure_balance(prev_balance, amount)?;
        let next_balance = prev_balance.checked_sub(amount).ok_or_else(overflow)?;

        let prev_total = storage::get_total_deposits(&e)?;
        let next_total = prev_total.checked_sub(amount).ok_or_else(overflow)?;

        let token_id = storage::get_deposit_token(&e)?;
        let token = soroban_sdk::token::Client::new(&e, &token_id);
        ensure_contract_balance(token.balance(&e.current_contract_address()), amount)?;
        token.transfer(&e.current_contract_address(), &to, &amount);

        storage::apply_user_reward_snapshot(&e, &to, &reward_snapshot);
        storage::set_user_balance(&e, &to, next_balance);
        storage::set_total_deposits(&e, next_total);

        events::emit_withdraw(&e, to, amount, next_balance);
        Ok(())
    }

    pub fn distribute_rewards(e: Env, amount: i128) -> Result<i128, VaultError> {
        storage::require_initialized(&e)?;
        validate_positive_amount(amount)?;
        let admin = storage::get_admin(&e)?;
        admin.require_auth();

        let total = storage::get_total_deposits(&e)?;
        if total <= 0 {
            return Err(BalanceError::NoDeposits.into());
        }

        let reward_token_id = storage::get_reward_token(&e)?;
        let reward_token = soroban_sdk::token::Client::new(&e, &reward_token_id);
        ensure_balance(reward_token.balance(&admin), amount)?;

        let scaled_amount = amount
            .checked_mul(REWARD_INDEX_SCALE)
            .ok_or(VaultError::from(ArithmeticError::RewardCalculationFailed))?;

        // Division by zero cannot happen here because `total > 0` is already
        // enforced above. The checked_mul guards against overflow.
        let increment = scaled_amount / total;

        let prev_idx = storage::get_reward_index(&e)?;
        let next_idx = prev_idx.checked_add(increment).ok_or_else(overflow)?;

        reward_token.transfer(&admin, &e.current_contract_address(), &amount);
        storage::set_reward_index(&e, next_idx);

        events::emit_distribute(&e, admin, amount, next_idx);
        Ok(next_idx)
    }

    pub fn claim_rewards(e: Env, user: Address) -> Result<i128, VaultError> {
        storage::require_initialized(&e)?;
        user.require_auth();

        let reward_snapshot = storage::preview_user_rewards(&e, &user)?;
        let amt = reward_snapshot.rewards;
        if amt <= 0 {
            return Ok(0);
        }

        let reward_token_id = storage::get_reward_token(&e)?;
        let reward_token = soroban_sdk::token::Client::new(&e, &reward_token_id);
        ensure_contract_balance(reward_token.balance(&e.current_contract_address()), amt)?;
        reward_token.transfer(&e.current_contract_address(), &user, &amt);

        storage::set_user_reward_index(&e, &user, reward_snapshot.reward_index);
        storage::set_user_rewards(&e, &user, 0);

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

// ---------------------------------------------------------------------------
// Internal validation helpers
// ---------------------------------------------------------------------------

/// Validates that `amount` is strictly positive.
///
/// Returns [`VaultError::NegativeAmount`] when `amount < 0` and
/// [`VaultError::InvalidAmount`] when `amount == 0`. This distinction gives
/// callers precise diagnostics about *why* their input was rejected.
fn validate_positive_amount(amount: i128) -> Result<(), VaultError> {
    if amount < 0 {
        return Err(ValidationError::NegativeAmount.into());
    }
    if amount == 0 {
        return Err(ValidationError::InvalidAmount.into());
    }
    Ok(())
}

fn validate_distinct_token_addresses(
    deposit_token: &Address,
    reward_token: &Address,
) -> Result<(), VaultError> {
    if deposit_token == reward_token {
        return Err(ValidationError::InvalidTokenConfiguration.into());
    }

    Ok(())
}

fn ensure_balance(balance: i128, requested_amount: i128) -> Result<(), VaultError> {
    if balance < requested_amount {
        return Err(BalanceError::InsufficientBalance.into());
    }

    Ok(())
}

fn ensure_contract_balance(balance: i128, requested_amount: i128) -> Result<(), VaultError> {
    if balance < requested_amount {
        return Err(BalanceError::InsufficientContractBalance.into());
    }

    Ok(())
}

fn overflow() -> VaultError {
    ArithmeticError::Overflow.into()
}

// TODO(reward-optimization): Consider a higher precision / rounding strategy for small totals.
// TODO(gas): Consider merging per-user keys (balance/index/rewards) into a single struct to reduce reads.
// TODO(security): Consider adding pausability or per-user deposit caps.
// TODO(governance): Introduce admin handover / multisig patterns.
// TODO(upgradeability): Evaluate upgrade patterns compatible with Soroban best practices.

#[cfg(test)]
mod test {
    use super::*;
    use crate::errors::ErrorCategory;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    // -----------------------------------------------------------------------
    // Happy-path tests
    // -----------------------------------------------------------------------

    #[test]
    fn deposit_withdraw_round_trip() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &400);
        assert_eq!(vault.balance(&user), 400);
        assert_eq!(vault.total_deposits(), 400);

        vault.withdraw(&user, &150);
        assert_eq!(vault.balance(&user), 250);
        assert_eq!(vault.total_deposits(), 250);

        let deposit_token_client = soroban_sdk::token::Client::new(&e, &deposit_token_id);
        assert_eq!(deposit_token_client.balance(&user), 750);
        assert_eq!(deposit_token_client.balance(&vault_id), 250);
    }

    #[test]
    fn rewards_are_proportional_and_claimable() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let alice = Address::generate(&e);
        let bob = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&alice, &1_000);
        deposit_token.mint(&bob, &1_000);
        reward_token.mint(&admin, &1_000);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&alice, &100);
        vault.deposit(&bob, &300);

        vault.distribute_rewards(&400);

        assert_eq!(vault.pending_rewards(&alice), 100);
        assert_eq!(vault.pending_rewards(&bob), 300);

        assert_eq!(vault.claim_rewards(&alice), 100);
        assert_eq!(vault.claim_rewards(&bob), 300);

        let reward_token_client = soroban_sdk::token::Client::new(&e, &reward_token_id);
        assert_eq!(reward_token_client.balance(&alice), 100);
        assert_eq!(reward_token_client.balance(&bob), 300);
        assert_eq!(reward_token_client.balance(&vault_id), 0);
    }

    // -----------------------------------------------------------------------
    // Validation error tests
    // -----------------------------------------------------------------------

    #[test]
    fn rejects_invalid_amounts() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Zero amounts should give InvalidAmount
        let err: VaultError = match vault.try_deposit(&user, &0) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InvalidAmount);

        let err: VaultError = match vault.try_withdraw(&user, &0) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InvalidAmount);

        let err: VaultError = match vault.try_distribute_rewards(&0) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InvalidAmount);
    }

    #[test]
    fn rejects_negative_amounts() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &1_000);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        // Negative amounts should give NegativeAmount
        let err: VaultError = match vault.try_deposit(&user, &-1) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::NegativeAmount);

        let err: VaultError = match vault.try_withdraw(&user, &-5) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::NegativeAmount);

        let err: VaultError = match vault.try_distribute_rewards(&-10) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::NegativeAmount);
    }

    #[test]
    fn rejects_invalid_token_configuration() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);

        let err: VaultError = match vault.try_initialize(&admin, &token_id, &token_id) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InvalidTokenConfiguration);

        // State must remain untouched after the failed initialization
        let err: VaultError = match vault.try_admin() {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::NotInitialized);
    }

    // -----------------------------------------------------------------------
    // Balance / insufficient-funds tests
    // -----------------------------------------------------------------------

    #[test]
    fn cannot_withdraw_more_than_balance() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &500);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &200);

        let err: VaultError = match vault.try_withdraw(&user, &201) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InsufficientBalance);
    }

    #[test]
    fn deposit_requires_available_user_tokens_without_mutating_rewards() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&user, &150);
        reward_token.mint(&admin, &200);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &100);
        vault.distribute_rewards(&60);

        e.as_contract(&vault_id, || {
            assert_eq!(storage::get_user_rewards(&e, &user).unwrap(), 0);
            assert_eq!(storage::get_user_reward_index(&e, &user).unwrap(), 0);
        });

        let err: VaultError = match vault.try_deposit(&user, &100) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InsufficientBalance);

        // Reward state must not be mutated by a failed deposit
        e.as_contract(&vault_id, || {
            assert_eq!(storage::get_user_rewards(&e, &user).unwrap(), 0);
            assert_eq!(storage::get_user_reward_index(&e, &user).unwrap(), 0);
        });
        assert_eq!(vault.pending_rewards(&user), 60);
        assert_eq!(vault.balance(&user), 100);
        assert_eq!(vault.total_deposits(), 100);
    }

    #[test]
    fn distribute_requires_deposits() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let reward_token = StellarAssetClient::new(&e, &reward_token_id);
        reward_token.mint(&admin, &1_000);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        let err: VaultError = match vault.try_distribute_rewards(&100) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::NoDeposits);
    }

    #[test]
    fn distribute_requires_available_admin_rewards() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        let reward_token = StellarAssetClient::new(&e, &reward_token_id);

        deposit_token.mint(&user, &500);
        reward_token.mint(&admin, &25);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);
        vault.deposit(&user, &200);

        let err: VaultError = match vault.try_distribute_rewards(&50) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InsufficientBalance);
        assert_eq!(vault.reward_index(), 0);
        assert_eq!(vault.pending_rewards(&user), 0);
    }

    // -----------------------------------------------------------------------
    // State-integrity tests
    // -----------------------------------------------------------------------

    #[test]
    fn initialization_is_one_time() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);

        vault.initialize(&admin, &deposit_token_id, &reward_token_id);
        let err: VaultError =
            match vault.try_initialize(&admin, &deposit_token_id, &reward_token_id) {
                Err(e) => match e {
                    Ok(ce) => ce,
                    Err(he) => panic!("host error: {:?}", he),
                },
                Ok(_) => panic!("expected contract error"),
            };
        assert_eq!(err, VaultError::AlreadyInitialized);
    }

    #[test]
    fn withdraw_does_not_mutate_state_on_error() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &500);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &300);

        // Snapshot state before the failing call
        let balance_before = vault.balance(&user);
        let total_before = vault.total_deposits();
        let token_balance_before =
            soroban_sdk::token::Client::new(&e, &deposit_token_id).balance(&user);

        // Attempt to over-withdraw
        let err: VaultError = match vault.try_withdraw(&user, &301) {
            Err(e) => match e {
                Ok(ce) => ce,
                Err(he) => panic!("host error: {:?}", he),
            },
            Ok(_) => panic!("expected contract error"),
        };
        assert_eq!(err, VaultError::InsufficientBalance);

        // State must be unchanged
        assert_eq!(vault.balance(&user), balance_before);
        assert_eq!(vault.total_deposits(), total_before);
        assert_eq!(
            soroban_sdk::token::Client::new(&e, &deposit_token_id).balance(&user),
            token_balance_before
        );
    }

    #[test]
    fn claim_with_no_pending_rewards_returns_zero() {
        let e = Env::default();
        e.mock_all_auths();

        let admin = Address::generate(&e);
        let user = Address::generate(&e);

        let deposit_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let reward_token_id = e
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let deposit_token = StellarAssetClient::new(&e, &deposit_token_id);
        deposit_token.mint(&user, &500);

        let vault_id = e.register(VaultContract, ());
        let vault = VaultContractClient::new(&e, &vault_id);
        vault.initialize(&admin, &deposit_token_id, &reward_token_id);

        vault.deposit(&user, &100);

        // No rewards distributed yet — claim should return 0 gracefully
        assert_eq!(vault.claim_rewards(&user), 0);
    }

    // -----------------------------------------------------------------------
    // Error metadata tests
    // -----------------------------------------------------------------------

    #[test]
    fn error_metadata_is_descriptive() {
        assert_eq!(
            VaultError::InvalidTokenConfiguration.message(),
            "deposit and reward token addresses must be different"
        );
        assert_eq!(
            VaultError::InvalidAmount.category(),
            ErrorCategory::Validation
        );
        assert_eq!(
            VaultError::InsufficientContractBalance.message(),
            "vault token balance is lower than the requested amount"
        );
        assert_eq!(VaultError::NoDeposits.category(), ErrorCategory::Balance);
    }

    #[test]
    fn error_metadata_covers_new_variants() {
        // NegativeAmount
        assert_eq!(
            VaultError::NegativeAmount.category(),
            ErrorCategory::Validation
        );
        assert_eq!(
            VaultError::NegativeAmount.message(),
            "amount must not be negative"
        );

        // InvalidAddress
        assert_eq!(
            VaultError::InvalidAddress.category(),
            ErrorCategory::Validation
        );
        assert_eq!(
            VaultError::InvalidAddress.message(),
            "provided address is invalid"
        );

        // RewardCalculationFailed
        assert_eq!(
            VaultError::RewardCalculationFailed.category(),
            ErrorCategory::Math
        );
        assert_eq!(
            VaultError::RewardCalculationFailed.message(),
            "reward calculation failed due to arithmetic error"
        );

        // Unauthorized (now has its own sub-error)
        assert_eq!(
            VaultError::Unauthorized.category(),
            ErrorCategory::Authorization
        );
        assert_eq!(
            VaultError::Unauthorized.message(),
            "caller is not authorized to perform this action"
        );
    }

    #[test]
    fn error_display_is_human_readable() {
        use alloc::format;

        let display = format!("{}", VaultError::InsufficientBalance);
        assert!(display.contains("available balance is lower than the requested amount"));

        let display = format!("{}", VaultError::NegativeAmount);
        assert!(display.contains("amount must not be negative"));

        let display = format!("{}", VaultError::RewardCalculationFailed);
        assert!(display.contains("reward calculation failed"));
    }
}
