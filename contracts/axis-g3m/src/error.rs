use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum G3mError {
    /// Account discriminator mismatch
    InvalidDiscriminator = 7000,
    /// Pool must contain 2-10 tokens
    InvalidTokenCount = 7001,
    /// Weights must sum to 10_000 basis points
    WeightsMismatch = 7002,
    /// Swap amount must be greater than zero
    ZeroAmount = 7003,
    /// Insufficient output amount (slippage exceeded)
    SlippageExceeded = 7004,
    /// Arithmetic overflow
    Overflow = 7005,
    /// Division by zero
    DivisionByZero = 7006,
    /// Invalid token index
    InvalidTokenIndex = 7007,
    /// Drift below threshold, rebalance not needed
    DriftBelowThreshold = 7008,
    /// Rebalance cooldown not elapsed
    CooldownActive = 7009,
    /// Pool is paused
    PoolPaused = 7010,
    /// G3M invariant violated after swap
    InvariantViolation = 7011,
    /// Invalid fee rate (max 10%)
    InvalidFeeRate = 7012,
    /// Duplicate token mint in basket
    DuplicateMint = 7013,
    /// Initial reserves must be non-zero
    ZeroReserve = 7014,
    /// Account already initialized
    AlreadyInitialized = 7015,
    /// Owner mismatch
    OwnerMismatch = 7016,
    /// Pool mismatch
    PoolMismatch = 7017,
    /// Post-rebalance per-token weight still exceeds drift threshold
    PerTokenDriftExceeded = 7018,
    /// Single-token reserve change exceeds maximum (attestation mode)
    ReserveChangeExceeded = 7019,
    /// Caller is not the pool authority
    Unauthorized = 7020,
    /// CPI target is not the Jupiter V6 program
    InvalidProgram = 7021,
}

impl From<G3mError> for ProgramError {
    fn from(e: G3mError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
