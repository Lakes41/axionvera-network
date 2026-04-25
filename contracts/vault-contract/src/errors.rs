use soroban_sdk::contracterror;

// ---------------------------------------------------------------------------
// Error categories – used by `ErrorInfo` for structured diagnostics.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ErrorCategory {
    Authorization,
    Balance,
    Math,
    State,
    Validation,
}

/// Rich metadata attached to every [`VaultError`] variant so that callers
/// (and off-chain tooling) can inspect *why* a transaction failed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ErrorInfo {
    pub category: ErrorCategory,
    pub message: &'static str,
}

// ---------------------------------------------------------------------------
// Domain-specific sub-error types.
//
// These give call-sites fine-grained variants that are then converted into the
// single on-chain [`VaultError`] via the `From` impls at the bottom.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum StateError {
    AlreadyInitialized,
    NotInitialized,
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
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AuthorizationError {
    Unauthorized,
}

// ---------------------------------------------------------------------------
// On-chain error enum – discriminants map to `u32` codes returned to callers.
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum VaultError {
    // State errors (1–2)
    AlreadyInitialized = 1,
    NotInitialized = 2,

    // Authorization errors (3)
    Unauthorized = 3,

    // Validation errors (4, 10–11)
    InvalidAmount = 4,
    NegativeAmount = 10,
    InvalidAddress = 11,
    InvalidTokenConfiguration = 8,

    // Balance errors (5, 7, 9)
    InsufficientBalance = 5,
    NoDeposits = 7,
    InsufficientContractBalance = 9,

    // Arithmetic errors (6, 12)
    MathOverflow = 6,
    RewardCalculationFailed = 12,
}

// ---------------------------------------------------------------------------
// Descriptive metadata for every variant.
// ---------------------------------------------------------------------------

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
            Self::NegativeAmount => ErrorInfo {
                category: ErrorCategory::Validation,
                message: "amount must not be negative",
            },
            Self::InvalidAddress => ErrorInfo {
                category: ErrorCategory::Validation,
                message: "provided address is invalid",
            },
            Self::InvalidTokenConfiguration => ErrorInfo {
                category: ErrorCategory::Validation,
                message: "deposit and reward token addresses must be different",
            },
            Self::InsufficientBalance => ErrorInfo {
                category: ErrorCategory::Balance,
                message: "available balance is lower than the requested amount",
            },
            Self::NoDeposits => ErrorInfo {
                category: ErrorCategory::Balance,
                message: "reward distribution requires at least one active deposit",
            },
            Self::InsufficientContractBalance => ErrorInfo {
                category: ErrorCategory::Balance,
                message: "vault token balance is lower than the requested amount",
            },
            Self::MathOverflow => ErrorInfo {
                category: ErrorCategory::Math,
                message: "arithmetic overflow or underflow detected",
            },
            Self::RewardCalculationFailed => ErrorInfo {
                category: ErrorCategory::Math,
                message: "reward calculation failed due to arithmetic error",
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

// ---------------------------------------------------------------------------
// Display – human-readable formatting for off-chain / logging use.
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// `From` conversions – domain sub-errors ⟶ `VaultError`.
// ---------------------------------------------------------------------------

impl From<StateError> for VaultError {
    fn from(error: StateError) -> Self {
        match error {
            StateError::AlreadyInitialized => Self::AlreadyInitialized,
            StateError::NotInitialized => Self::NotInitialized,
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
        }
    }
}

impl From<AuthorizationError> for VaultError {
    fn from(error: AuthorizationError) -> Self {
        match error {
            AuthorizationError::Unauthorized => Self::Unauthorized,
        }
    }
}
