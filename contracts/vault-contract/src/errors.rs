use soroban_sdk::contracterror;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ErrorCategory {
    Authorization,
    Balance,
    Math,
    State,
    Validation,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ErrorInfo {
    pub category: ErrorCategory,
    pub message: &'static str,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StateError {
    AlreadyInitialized,
    NotInitialized,
    InvalidState,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    InvalidAmount,
    NegativeAmount,
    InvalidAddress,
    InvalidTokenConfiguration,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BalanceError {
    InsufficientBalance,
    InsufficientContractBalance,
    NoDeposits,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ArithmeticError {
    Overflow,
    RewardCalculationFailed,
    ZeroRewardIncrement,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationError {
    Unauthorized,
    ReentrancyDetected,
}

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
    InvalidTokenConfiguration = 8,
    InsufficientContractBalance = 9,
    NegativeAmount = 10,
    InvalidAddress = 11,
    RewardCalculationFailed = 12,

    // Additional errors
    ReentrancyDetected = 13,
    InvalidState = 14,
    ZeroRewardIncrement = 15,
}

impl VaultError {
    pub const fn info(self) -> ErrorInfo {
        match self {
            Self::AlreadyInitialized => ErrorInfo {
                category: ErrorCategory::State,
                message: "vault has already been initialized",
            },
            Self::NotInitialized => ErrorInfo {
                category: ErrorCategory::State,
                message: "vault has not been initialized",
            },
            Self::Unauthorized => ErrorInfo {
                category: ErrorCategory::Authorization,
                message: "caller is not authorized to perform this action",
            },
            Self::InvalidAmount => ErrorInfo {
                category: ErrorCategory::Validation,
                message: "amount must be greater than zero",
            },
            Self::InsufficientBalance => ErrorInfo {
                category: ErrorCategory::Balance,
                message: "available balance is lower than the requested amount",
            },
            Self::MathOverflow => ErrorInfo {
                category: ErrorCategory::Math,
                message: "arithmetic overflow or underflow detected",
            },
            Self::NoDeposits => ErrorInfo {
                category: ErrorCategory::Balance,
                message: "reward distribution requires at least one active deposit",
            },
            Self::InvalidTokenConfiguration => ErrorInfo {
                category: ErrorCategory::Validation,
                message: "deposit and reward token addresses must be different",
            },
            Self::InsufficientContractBalance => ErrorInfo {
                category: ErrorCategory::Balance,
                message: "vault token balance is lower than the requested amount",
            },
            Self::NegativeAmount => ErrorInfo {
                category: ErrorCategory::Validation,
                message: "amount must not be negative",
            },
            Self::InvalidAddress => ErrorInfo {
                category: ErrorCategory::Validation,
                message: "provided address is invalid",
            },
            Self::RewardCalculationFailed => ErrorInfo {
                category: ErrorCategory::Math,
                message: "reward calculation failed due to checked arithmetic",
            },
            Self::ReentrancyDetected => ErrorInfo {
                category: ErrorCategory::Authorization,
                message: "reentrant contract call detected",
            },
            Self::InvalidState => ErrorInfo {
                category: ErrorCategory::State,
                message: "vault state is internally inconsistent",
            },
            Self::ZeroRewardIncrement => ErrorInfo {
                category: ErrorCategory::Math,
                message: "reward distribution rounded down to zero",
            },
            Self::ReentrancyDetected => ErrorInfo {
                category: ErrorCategory::State,
                message: "reentrancy detected",
            },
            Self::InvalidState => ErrorInfo {
                category: ErrorCategory::State,
                message: "invalid contract state",
            },
            Self::ZeroRewardIncrement => ErrorInfo {
                category: ErrorCategory::Math,
                message: "reward increment is zero",
            },
        }
    }

    pub const fn category(self) -> ErrorCategory {
        self.info().category
    }

    pub const fn message(self) -> &'static str {
        self.info().message
    }
}

impl core::fmt::Display for VaultError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let info = self.info();
        write!(f, "VaultError::{:?}: {}", self, info.message)
    }
}

impl core::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<StateError> for VaultError {
    fn from(error: StateError) -> Self {
        match error {
            StateError::AlreadyInitialized => Self::AlreadyInitialized,
            StateError::NotInitialized => Self::NotInitialized,
            StateError::InvalidState => Self::InvalidState,
        }
    }
}

impl From<ValidationError> for VaultError {
    fn from(error: ValidationError) -> Self {
        match error {
            ValidationError::InvalidAmount => Self::InvalidAmount,
            ValidationError::NegativeAmount => Self::NegativeAmount,
            ValidationError::InvalidAddress => Self::InvalidAddress,
            ValidationError::InvalidTokenConfiguration => Self::InvalidTokenConfiguration,
        }
    }
}

impl From<BalanceError> for VaultError {
    fn from(error: BalanceError) -> Self {
        match error {
            BalanceError::InsufficientBalance => Self::InsufficientBalance,
            BalanceError::InsufficientContractBalance => Self::InsufficientContractBalance,
            BalanceError::NoDeposits => Self::NoDeposits,
        }
    }
}

impl From<ArithmeticError> for VaultError {
    fn from(error: ArithmeticError) -> Self {
        match error {
            ArithmeticError::Overflow => Self::MathOverflow,
            ArithmeticError::RewardCalculationFailed => Self::RewardCalculationFailed,
            ArithmeticError::ZeroRewardIncrement => Self::ZeroRewardIncrement,
        }
    }
}

impl From<AuthorizationError> for VaultError {
    fn from(error: AuthorizationError) -> Self {
        match error {
            AuthorizationError::Unauthorized => Self::Unauthorized,
            AuthorizationError::ReentrancyDetected => Self::ReentrancyDetected,
        }
    }
}
