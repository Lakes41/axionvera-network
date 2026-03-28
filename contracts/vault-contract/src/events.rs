use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

const EVT_INIT: Symbol = symbol_short!("init");
const EVT_DEPOSIT: Symbol = symbol_short!("deposit");
const EVT_WITHDRAW: Symbol = symbol_short!("withdraw");
const EVT_DISTRIBUTE: Symbol = symbol_short!("distrib");
const EVT_CLAIM: Symbol = symbol_short!("claim");

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializeEvent {
    pub admin: Address,
    pub deposit_token: Address,
    pub reward_token: Address,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DepositEvent {
    pub user: Address,
    pub amount: i128,
    pub new_balance: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawEvent {
    pub user: Address,
    pub amount: i128,
    pub new_balance: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributeRewardsEvent {
    pub caller: Address,
    pub amount: i128,
    pub reward_index: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimRewardsEvent {
    pub user: Address,
    pub amount: i128,
    pub timestamp: u64,
}

pub fn emit_initialize(e: &Env, admin: Address, deposit_token: Address, reward_token: Address) {
    e.events().publish(
        (EVT_INIT,),
        InitializeEvent {
            admin,
            deposit_token,
            reward_token,
            timestamp: e.ledger().timestamp(),
        },
    );
}

pub fn emit_deposit(e: &Env, user: Address, amount: i128, new_balance: i128) {
    e.events().publish(
        (EVT_DEPOSIT,),
        DepositEvent {
            user,
            amount,
            new_balance,
            timestamp: e.ledger().timestamp(),
        },
    );
}

pub fn emit_withdraw(e: &Env, user: Address, amount: i128, new_balance: i128) {
    e.events().publish(
        (EVT_WITHDRAW,),
        WithdrawEvent {
            user,
            amount,
            new_balance,
            timestamp: e.ledger().timestamp(),
        },
    );
}

pub fn emit_distribute(e: &Env, caller: Address, amount: i128, reward_index: i128) {
    e.events().publish(
        (EVT_DISTRIBUTE,),
        DistributeRewardsEvent {
            caller,
            amount,
            reward_index,
            timestamp: e.ledger().timestamp(),
        },
    );
}

pub fn emit_claim(e: &Env, user: Address, amount: i128) {
    e.events().publish(
        (EVT_CLAIM,),
        ClaimRewardsEvent {
            user,
            amount,
            timestamp: e.ledger().timestamp(),
        },
    );
}
