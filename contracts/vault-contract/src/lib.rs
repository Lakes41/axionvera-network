#![no_std]
extern crate alloc;

mod errors;
mod events;
mod storage;

use soroban_sdk::{contract, contractimpl, Address, Env};

use crate::errors::{ArithmeticError, BalanceError, StateError, ValidationError, VaultError};

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

        storage::initialize_state(&e, &admin, &deposit_token, &reward_token);

        events::emit_initialize(&e, admin, deposit_token, reward_token);
        Ok(())
    }

    /// Deposits tokens into the vault and accrues pending rewards before updating balance.
    /// This ensures users receive rewards based on their old balance up to this point.
    pub fn deposit(e: Env, from: Address, amount: i128) -> Result<(), VaultError> {
        validate_positive_amount(amount)?;
        from.require_auth();

        with_non_reentrant(&e, || {
            let (state, position) = storage::store_deposit(&e, &from, amount)?;
            let token = soroban_sdk::token::Client::new(&e, &state.deposit_token);
            token.transfer(&from, &e.current_contract_address(), &amount);
            events::emit_deposit(&e, from, amount, position.balance);
            Ok(())
        })
    }

    /// Withdraws tokens from the vault and accrues pending rewards before updating balance.
    /// This function is isolated from reward claiming - it only handles the deposit token.
    /// If the reward token contract fails, users can still withdraw their deposits.
    pub fn withdraw(e: Env, to: Address, amount: i128) -> Result<(), VaultError> {
        validate_positive_amount(amount)?;
        to.require_auth();

        with_non_reentrant(&e, || {
            let (state, position) = storage::store_withdraw(&e, &to, amount)?;
            let token = soroban_sdk::token::Client::new(&e, &state.deposit_token);
            token.transfer(&e.current_contract_address(), &to, &amount);

            events::emit_withdraw(&e, to, amount, position.balance);
            Ok(())
        })
    }

    /// Distributes rewards to all depositors by updating the global reward index.
    /// Does not immediately transfer rewards to users - they accrue lazily.
    pub fn distribute_rewards(e: Env, amount: i128) -> Result<i128, VaultError> {
        validate_positive_amount(amount)?;

        let state = storage::get_state(&e)?;
        let admin = state.admin;
        let reward_token = state.reward_token;
        admin.require_auth();

        with_non_reentrant(&e, || {
            let next_idx = storage::store_reward_distribution(&e, amount)?.reward_index;
            let reward_token_client = soroban_sdk::token::Client::new(&e, &reward_token);
            reward_token_client.transfer(&admin, &e.current_contract_address(), &amount);
            events::emit_distribute(&e, admin, amount, next_idx);
            Ok(next_idx)
        })
    }

    /// Claims accrued rewards for a user.
    /// Isolated from withdraw to ensure exit liquidity is always available.
    pub fn claim_rewards(e: Env, user: Address) -> Result<i128, VaultError> {
        user.require_auth();

        with_non_reentrant(&e, || {
            let amt = storage::store_claimable_rewards(&e, &user)?;
            if amt <= 0 {
                return Ok(0);
            }

            let reward_token_id = storage::get_reward_token(&e)?;
            let reward_token = soroban_sdk::token::Client::new(&e, &reward_token_id);
            ensure_contract_balance(reward_token.balance(&e.current_contract_address()), amt)?;
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
