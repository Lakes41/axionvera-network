use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InsufficientBalance = 5,
    MathOverflow = 6,
    NoDeposits = 7,
    InvalidConfiguration = 8,
    ReentrancyDetected = 9,
    InvalidState = 10,
    ZeroRewardIncrement = 11,
}
