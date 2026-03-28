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
    Initialized,
    ReentrancyGuard,
    Admin,
    DepositToken,
    RewardToken,
    TotalDeposits,
    RewardIndex,
    UserBalance(Address),
    UserRewardIndex(Address),
    UserRewards(Address),
}

pub fn is_initialized(e: &Env) -> bool {
    e.storage().instance().has(&DataKey::Initialized)
}

pub fn require_initialized(e: &Env) -> Result<(), VaultError> {
    if !is_initialized(e) {
        return Err(VaultError::NotInitialized);
    }
    bump_instance_ttl(e);
    Ok(())
}

pub fn set_initialized(e: &Env) {
    e.storage().instance().set(&DataKey::Initialized, &true);
    bump_instance_ttl(e);
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

pub fn set_admin(e: &Env, admin: &Address) {
    e.storage().instance().set(&DataKey::Admin, admin);
    bump_instance_ttl(e);
}

pub fn get_admin(e: &Env) -> Result<Address, VaultError> {
    require_initialized(e)?;
    e.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(VaultError::NotInitialized)
}

pub fn set_deposit_token(e: &Env, token: &Address) {
    e.storage().instance().set(&DataKey::DepositToken, token);
    bump_instance_ttl(e);
}

pub fn get_deposit_token(e: &Env) -> Result<Address, VaultError> {
    require_initialized(e)?;
    e.storage()
        .instance()
        .get(&DataKey::DepositToken)
        .ok_or(VaultError::NotInitialized)
}

pub fn set_reward_token(e: &Env, token: &Address) {
    e.storage().instance().set(&DataKey::RewardToken, token);
    bump_instance_ttl(e);
}

pub fn get_reward_token(e: &Env) -> Result<Address, VaultError> {
    require_initialized(e)?;
    e.storage()
        .instance()
        .get(&DataKey::RewardToken)
        .ok_or(VaultError::NotInitialized)
}

pub fn get_total_deposits(e: &Env) -> Result<i128, VaultError> {
    require_initialized(e)?;
    Ok(e.storage()
        .instance()
        .get(&DataKey::TotalDeposits)
        .unwrap_or(0_i128))
}

pub fn set_total_deposits(e: &Env, total: i128) {
    e.storage().instance().set(&DataKey::TotalDeposits, &total);
    bump_instance_ttl(e);
}

pub fn get_reward_index(e: &Env) -> Result<i128, VaultError> {
    require_initialized(e)?;
    Ok(e.storage()
        .instance()
        .get(&DataKey::RewardIndex)
        .unwrap_or(0_i128))
}

pub fn set_reward_index(e: &Env, idx: i128) {
    e.storage().instance().set(&DataKey::RewardIndex, &idx);
    bump_instance_ttl(e);
}

pub fn get_user_balance(e: &Env, user: &Address) -> Result<i128, VaultError> {
    require_initialized(e)?;
    let key = DataKey::UserBalance(user.clone());
    let bal = e.storage().persistent().get(&key).unwrap_or(0_i128);
    bump_persistent_ttl_if_present(e, &key);
    Ok(bal)
}

pub fn set_user_balance(e: &Env, user: &Address, balance: i128) {
    let key = DataKey::UserBalance(user.clone());
    if balance == 0 {
        e.storage().persistent().remove(&key);
    } else {
        e.storage().persistent().set(&key, &balance);
        bump_persistent_ttl(e, &key);
    }
}

pub fn get_user_reward_index(e: &Env, user: &Address) -> Result<i128, VaultError> {
    require_initialized(e)?;
    let key = DataKey::UserRewardIndex(user.clone());
    let idx = e.storage().persistent().get(&key).unwrap_or(0_i128);
    bump_persistent_ttl_if_present(e, &key);
    Ok(idx)
}

pub fn set_user_reward_index(e: &Env, user: &Address, idx: i128) {
    let key = DataKey::UserRewardIndex(user.clone());
    if idx == 0 {
        e.storage().persistent().remove(&key);
    } else {
        e.storage().persistent().set(&key, &idx);
        bump_persistent_ttl(e, &key);
    }
}

pub fn get_user_rewards(e: &Env, user: &Address) -> Result<i128, VaultError> {
    require_initialized(e)?;
    let key = DataKey::UserRewards(user.clone());
    let amt = e.storage().persistent().get(&key).unwrap_or(0_i128);
    bump_persistent_ttl_if_present(e, &key);
    Ok(amt)
}

pub fn set_user_rewards(e: &Env, user: &Address, amt: i128) {
    let key = DataKey::UserRewards(user.clone());
    if amt == 0 {
        e.storage().persistent().remove(&key);
    } else {
        e.storage().persistent().set(&key, &amt);
        bump_persistent_ttl(e, &key);
    }
}

pub fn accrue_user_rewards(e: &Env, user: &Address) -> Result<(), VaultError> {
    let global_idx = get_reward_index(e)?;
    let user_idx = get_user_reward_index(e, user)?;
    if global_idx == user_idx {
        return Ok(());
    }

    let balance = get_user_balance(e, user)?;
    if balance > 0 {
        let delta = global_idx
            .checked_sub(user_idx)
            .ok_or(VaultError::MathOverflow)?;

        let accrued =
            balance.checked_mul(delta).ok_or(VaultError::MathOverflow)? / REWARD_INDEX_SCALE;

        if accrued > 0 {
            let current = get_user_rewards(e, user)?;
            let next = current
                .checked_add(accrued)
                .ok_or(VaultError::MathOverflow)?;
            set_user_rewards(e, user, next);
        }
    }

    set_user_reward_index(e, user, global_idx);
    Ok(())
}

pub fn pending_user_rewards_view(e: &Env, user: &Address) -> Result<i128, VaultError> {
    require_initialized(e)?;
    let global_idx = get_reward_index(e)?;
    let user_idx = get_user_reward_index(e, user)?;
    let current = get_user_rewards(e, user)?;
    if global_idx == user_idx {
        return Ok(current);
    }

    let balance = get_user_balance(e, user)?;
    if balance == 0 {
        return Ok(current);
    }

    let delta = global_idx
        .checked_sub(user_idx)
        .ok_or(VaultError::MathOverflow)?;
    let accrued = balance.checked_mul(delta).ok_or(VaultError::MathOverflow)? / REWARD_INDEX_SCALE;
    Ok(current
        .checked_add(accrued)
        .ok_or(VaultError::MathOverflow)?)
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
