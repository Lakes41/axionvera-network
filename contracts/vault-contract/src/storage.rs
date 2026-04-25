use soroban_sdk::{contracttype, Address, Env};

use crate::errors::{ArithmeticError, AuthorizationError, StateError, VaultError};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserRewardSnapshot {
    pub reward_index: i128,
    pub rewards: i128,
}

pub fn is_initialized(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::State)
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
        .get::<_, bool>(&DataKey::ReentrancyGuard)
        .unwrap_or(false)
    {
        return Err(AuthorizationError::ReentrancyDetected.into());
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
    let state = e
        .storage()
        .instance()
        .get(&DataKey::State)
        .ok_or(StateError::NotInitialized)?;
    bump_instance_ttl(e);
    Ok(state)
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

pub fn set_total_deposits(e: &Env, total: i128) {
    if let Ok(mut state) = get_state(e) {
        state.total_deposits = total;
        set_state(e, &state);
    }
}

pub fn get_reward_index(e: &Env) -> Result<i128, VaultError> {
    Ok(get_state(e)?.reward_index)
}

pub fn get_user_position(e: &Env, user: &Address) -> Result<UserPosition, VaultError> {
    require_initialized(e)?;
    bump_instance_ttl(e);
    Ok(get_user_position_unchecked(e, user))
}

fn get_user_position_unchecked(e: &Env, user: &Address) -> UserPosition {
    let key = DataKey::User(user.clone());
    let position = e.storage().persistent().get(&key);
    if let Some(existing) = position {
        bump_persistent_ttl(e, &key);
        existing
    } else {
        UserPosition::default()
    }
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

pub fn set_user_balance(e: &Env, user: &Address, balance: i128) {
    let mut position = get_user_position_unchecked(e, user);
    position.balance = balance;
    set_user_position(e, user, &position);
}

pub fn get_user_reward_index(e: &Env, user: &Address) -> Result<i128, VaultError> {
    Ok(get_user_position(e, user)?.reward_index)
}

pub fn set_user_reward_index(e: &Env, user: &Address, index: i128) {
    let mut position = get_user_position_unchecked(e, user);
    position.reward_index = index;
    set_user_position(e, user, &position);
}

pub fn get_user_rewards(e: &Env, user: &Address) -> Result<i128, VaultError> {
    Ok(get_user_position(e, user)?.rewards)
}

pub fn set_user_rewards(e: &Env, user: &Address, rewards: i128) {
    let mut position = get_user_position_unchecked(e, user);
    position.rewards = rewards;
    set_user_position(e, user, &position);
}

pub fn store_deposit(
    e: &Env,
    user: &Address,
    amount: i128,
) -> Result<(VaultState, UserPosition), VaultError> {
    let mut state = get_state(e)?;
    let mut position = get_user_position_unchecked(e, user);
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
    let mut position = get_user_position_unchecked(e, user);
    accrue_position_rewards(&state, &mut position)?;

    if position.balance < amount {
        return Err(VaultError::InsufficientBalance);
    }
    if state.total_deposits < amount {
        return Err(StateError::InvalidState.into());
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
    let increment = checked_reward_index_increment(amount, state.total_deposits)?;

    state.reward_index = state
        .reward_index
        .checked_add(increment)
        .ok_or(VaultError::MathOverflow)?;

    set_state(e, &state);
    Ok(state)
}

pub fn store_claimable_rewards(e: &Env, user: &Address) -> Result<i128, VaultError> {
    let state = get_state(e)?;
    let mut position = get_user_position_unchecked(e, user);
    accrue_position_rewards(&state, &mut position)?;

    let claimable = position.rewards;
    position.rewards = 0;
    set_user_position(e, user, &position);

    Ok(claimable)
}

pub fn preview_user_rewards(e: &Env, user: &Address) -> Result<UserRewardSnapshot, VaultError> {
    if !is_initialized(e) {
        return Err(VaultError::NotInitialized);
    }

    let global_idx = get_reward_index(e)?;
    let user_idx = get_user_reward_index(e, user)?;
    let current_rewards = get_user_rewards(e, user)?;
    if global_idx == user_idx {
        return Ok(UserRewardSnapshot {
            reward_index: user_idx,
            rewards: current_rewards,
        });
    }

    if state.reward_index == position.reward_index || position.balance == 0 {
        return Ok(UserRewardSnapshot {
            reward_index: state.reward_index,
            rewards: position.rewards,
        });
    }

    let delta = state
        .reward_index
        .checked_sub(position.reward_index)
        .ok_or(VaultError::MathOverflow)?;
    let accrued = checked_accrued_rewards(position.balance, delta)?;
    let rewards = position
        .rewards
        .checked_add(accrued)
        .ok_or(VaultError::MathOverflow)?;

    Ok(UserRewardSnapshot {
        reward_index: state.reward_index,
        rewards,
    })
}

pub(crate) fn checked_reward_index_increment(
    amount: i128,
    total_deposits: i128,
) -> Result<i128, VaultError> {
    if total_deposits <= 0 {
        return Err(VaultError::NoDeposits);
    }

    let scaled = amount
        .checked_mul(REWARD_INDEX_SCALE)
        .ok_or(VaultError::MathOverflow)?;
    let increment = scaled
        .checked_div(total_deposits)
        .ok_or(VaultError::from(ArithmeticError::RewardCalculationFailed))?;

    if increment <= 0 {
        return Err(VaultError::from(ArithmeticError::ZeroRewardIncrement));
    }

    Ok(increment)
}

pub fn pending_user_rewards_view(e: &Env, user: &Address) -> Result<i128, VaultError> {
    Ok(preview_user_rewards(e, user)?.rewards)
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
        let accrued = checked_accrued_rewards(position.balance, delta)?;

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

fn require_initialized(e: &Env) -> Result<(), VaultError> {
    if is_initialized(e) {
        Ok(())
    } else {
        Err(StateError::NotInitialized.into())
    }
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
