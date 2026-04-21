use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum VaultError {
    InvalidDiscriminator = 9000,
    AlreadyInitialized = 9001,
    InvalidBasketSize = 9002,
    WeightsMismatch = 9003,
    ZeroDeposit = 9004,
    InsufficientBalance = 9005,
    DivisionByZero = 9006,
    Overflow = 9007,
    OwnerMismatch = 9008,
    MintMismatch = 9009,
    InvalidTickerLength = 9010,
    DuplicateMint = 9011,
    PoolPaused = 9012,
    VaultMismatch = 9013,
    InvalidProgramOwner = 9014,
    SlippageExceeded = 9015,
    NavDeviationExceeded = 9016,
    TreasuryMismatch = 9017,
    InsufficientFirstDeposit = 9018,
    InvalidTicker = 9019,
    InvalidName = 9020,
    SweepForbidden = 9021,
    NothingToSweep = 9022,
    TreasuryNotApproved = 9023,
    /// DepositSol / WithdrawSol scaffolding shipped; native Jupiter CPI
    /// implementation is a follow-up after design review. Issue #36.
    NotYetImplemented = 9024,
    /// DepositSol / WithdrawSol restrict basket size to keep CU + tx
    /// account count within the 1.4M / versioned-tx envelopes. 5-leg
    /// baskets must use the client-bundled flow from PR #42. Issue #36.
    BasketTooLargeForOnchainSol = 9025,
}

impl From<VaultError> for ProgramError {
    fn from(e: VaultError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
