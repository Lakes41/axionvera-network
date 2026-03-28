use soroban_sdk::{contracttype, Address, Env};

use crate::errors::VaultError;

pub const REWARD_INDEX_SCALE: i128 = 1_000_000_000_000_000_000;

const INSTANCE_TTL_THRESHOLD: u32 = 100;
const INSTANCE_TTL_EXTEND_TO: u32 = 1_000;

const PERSISTENT_TTL_THRESHOLD: u32 = 100;
const PERSISTENT_TTL_EXTEND_TO: u32 = 10_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    ReentrancyGuard,
    State,
    User(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultState {
    pub admin: Address,
    pub deposit_token: Address,
    pub reward_token: Address,
    pub total_deposits: i128,
    pub reward_index: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct UserPosition {
    pub balance: i128,
    pub reward_index: i128,
    pub rewards: i128,
}

pub fn is_initialized(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::State)
}

pub fn require_initialized(e: &Env) -> Result<(), VaultError> {
    if !is_initialized(e) {
        return Err(VaultError::NotInitialized);
    }
    bump_instance_ttl(e);
    Ok(())
}

pub fn initialize_state(
    e: &Env,
    admin: &Address,
    deposit_token: &Address,
    reward_token: &Address,
) {
    let state = VaultState {
        admin: admin.clone(),
        deposit_token: deposit_token.clone(),
        reward_token: reward_token.clone(),
        total_deposits: 0,
        reward_index: 0,
    };
    set_state(e, &state);
}

pub fn enter_non_reentrant(e: &Env) -> Result<(), VaultError> {
    if e.storage()
        .instance()
        .get(&DataKey::ReentrancyGuard)
        .unwrap_or(false)
    {
        return Err(VaultError::ReentrancyDetected);
    }

    e.storage().instance().set(&DataKey::ReentrancyGuard, &true);
    bump_instance_ttl(e);
    Ok(())
}

pub fn exit_non_reentrant(e: &Env) {
    e.storage().instance().remove(&DataKey::ReentrancyGuard);
    bump_instance_ttl(e);
}

pub fn get_state(e: &Env) -> Result<VaultState, VaultError> {
    require_initialized(e)?;
    e.storage()
        .instance()
        .get(&DataKey::State)
        .ok_or(VaultError::NotInitialized)
}

pub fn set_state(e: &Env, state: &VaultState) {
    e.storage().instance().set(&DataKey::State, state);
    bump_instance_ttl(e);
}

pub fn get_admin(e: &Env) -> Result<Address, VaultError> {
    Ok(get_state(e)?.admin)
}

pub fn get_deposit_token(e: &Env) -> Result<Address, VaultError> {
    Ok(get_state(e)?.deposit_token)
}

pub fn get_reward_token(e: &Env) -> Result<Address, VaultError> {
    Ok(get_state(e)?.reward_token)
}

pub fn get_total_deposits(e: &Env) -> Result<i128, VaultError> {
    Ok(get_state(e)?.total_deposits)
}

pub fn get_reward_index(e: &Env) -> Result<i128, VaultError> {
    Ok(get_state(e)?.reward_index)
}

pub fn get_user_position(e: &Env, user: &Address) -> Result<UserPosition, VaultError> {
    require_initialized(e)?;
    let key = DataKey::User(user.clone());
    let bal = e.storage().persistent().get(&key).unwrap_or_default();
    bump_persistent_ttl_if_present(e, &key);
    Ok(bal)
}

pub fn set_user_position(e: &Env, user: &Address, position: &UserPosition) {
    let key = DataKey::User(user.clone());
    if position == &UserPosition::default() {
        e.storage().persistent().remove(&key);
    } else {
        e.storage().persistent().set(&key, position);
        bump_persistent_ttl(e, &key);
    }
}

pub fn get_user_balance(e: &Env, user: &Address) -> Result<i128, VaultError> {
    Ok(get_user_position(e, user)?.balance)
}

pub fn store_deposit(
    e: &Env,
    user: &Address,
    amount: i128,
) -> Result<(VaultState, UserPosition), VaultError> {
    let mut state = get_state(e)?;
    let mut position = get_user_position(e, user)?;
    accrue_position_rewards(&state, &mut position)?;

    position.balance = position
        .balance
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;
    state.total_deposits = state
        .total_deposits
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;

    set_state(e, &state);
    set_user_position(e, user, &position);
    Ok((state, position))
}

pub fn store_withdraw(
    e: &Env,
    user: &Address,
    amount: i128,
) -> Result<(VaultState, UserPosition), VaultError> {
    let mut state = get_state(e)?;
    let mut position = get_user_position(e, user)?;
    accrue_position_rewards(&state, &mut position)?;

    if position.balance < amount {
        return Err(VaultError::InsufficientBalance);
    }
    if state.total_deposits < amount {
        return Err(VaultError::InvalidState);
    }

    position.balance = position
        .balance
        .checked_sub(amount)
        .ok_or(VaultError::MathOverflow)?;
    state.total_deposits = state
        .total_deposits
        .checked_sub(amount)
        .ok_or(VaultError::MathOverflow)?;

    set_state(e, &state);
    set_user_position(e, user, &position);
    Ok((state, position))
}

pub fn store_reward_distribution(e: &Env, amount: i128) -> Result<VaultState, VaultError> {
    let mut state = get_state(e)?;
    if state.total_deposits <= 0 {
        return Err(VaultError::NoDeposits);
    }

    let increment = amount
        .checked_mul(REWARD_INDEX_SCALE)
        .ok_or(VaultError::MathOverflow)?
        / state.total_deposits;
    if increment <= 0 {
        return Err(VaultError::ZeroRewardIncrement);
    }

    state.reward_index = state
        .reward_index
        .checked_add(increment)
        .ok_or(VaultError::MathOverflow)?;

    set_state(e, &state);
    Ok(state)
}

pub fn store_claimable_rewards(e: &Env, user: &Address) -> Result<i128, VaultError> {
    let state = get_state(e)?;
    let mut position = get_user_position(e, user)?;
    accrue_position_rewards(&state, &mut position)?;

    let claimable = position.rewards;
    if claimable > 0 {
        position.rewards = 0;
        set_user_position(e, user, &position);
    } else if position.reward_index != state.reward_index {
        set_user_position(e, user, &position);
    }

    Ok(claimable)
}

pub fn pending_user_rewards_view(e: &Env, user: &Address) -> Result<i128, VaultError> {
    let state = get_state(e)?;
    let mut position = get_user_position(e, user)?;
    accrue_position_rewards(&state, &mut position)?;
    Ok(position.rewards)
}

fn accrue_position_rewards(
    state: &VaultState,
    position: &mut UserPosition,
) -> Result<(), VaultError> {
    if state.reward_index == position.reward_index {
        return Ok(());
    }

    if position.balance > 0 {
        let delta = state
            .reward_index
            .checked_sub(position.reward_index)
            .ok_or(VaultError::MathOverflow)?;

        let accrued = position
            .balance
            .checked_mul(delta)
            .ok_or(VaultError::MathOverflow)?
            / REWARD_INDEX_SCALE;

        if accrued > 0 {
            position.rewards = position
                .rewards
                .checked_add(accrued)
                .ok_or(VaultError::MathOverflow)?;
        }
    }

    position.reward_index = state.reward_index;
    Ok(())
}

fn bump_instance_ttl(e: &Env) {
    e.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

fn bump_persistent_ttl(e: &Env, key: &DataKey) {
    e.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
}

fn bump_persistent_ttl_if_present(e: &Env, key: &DataKey) {
    if e.storage().persistent().has(key) {
        bump_persistent_ttl(e, key);
    }
}
