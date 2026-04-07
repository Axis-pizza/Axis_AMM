use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Pfda3Error {
    InvalidDiscriminator = 8000,
    ReentrancyDetected = 8001,
    BatchWindowNotEnded = 8002,
    BatchAlreadyCleared = 8003,
    TicketAlreadyClaimed = 8004,
    BatchNotCleared = 8005,
    SlippageExceeded = 8006,
    InvalidSwapInput = 8007,
    Overflow = 8008,
    InvalidWeight = 8009,
    BatchIdMismatch = 8010,
    PoolMismatch = 8011,
    OwnerMismatch = 8012,
    ClearingPriceFailed = 8013,
    InvalidWindowSlots = 8014,
    AlreadyInitialized = 8015,
    InvalidTokenIndex = 8016,
    DivisionByZero = 8017,
    PoolPaused = 8018,
    TreasuryMismatch = 8019,
    OracleInvalid = 8020,
    OraclePriceNegative = 8021,
    OracleStale = 8022,
    OracleInsufficientSamples = 8023,
    BidTooLow = 8024,
    VaultMismatch = 8025,
    MintMismatch = 8026,
    BidWithoutTreasury = 8027,
    OracleOwnerMismatch = 8028,
    /// Post-clearing reserve insufficient for worst-case outflows
    ReserveInsufficient = 8029,
    /// Post-claim reserve product invariant violated
    InvariantViolation = 8030,
    /// Bid is disproportionately large relative to batch volume
    BidExcessive = 8031,
}

impl From<Pfda3Error> for ProgramError {
    fn from(e: Pfda3Error) -> Self {
        ProgramError::Custom(e as u32)
    }
}
