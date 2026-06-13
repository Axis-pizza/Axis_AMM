use pinocchio::program_error::ProgramError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum PfmmError {
    /// Account discriminator mismatch
    InvalidDiscriminator = 6000,
    /// Reentrancy detected
    ReentrancyDetected = 6001,
    /// Batch window not yet ended
    BatchWindowNotEnded = 6002,
    /// Batch already cleared
    BatchAlreadyCleared = 6003,
    /// Ticket already claimed
    TicketAlreadyClaimed = 6004,
    /// Batch not yet cleared
    BatchNotCleared = 6005,
    /// Slippage exceeded (output below minimum)
    SlippageExceeded = 6006,
    /// Invalid input: both amount_in_a and amount_in_b are non-zero or both zero
    InvalidSwapInput = 6007,
    /// Arithmetic overflow
    Overflow = 6008,
    /// Invalid weight: must be between 0 and 1_000_000
    InvalidWeight = 6009,
    /// Batch ID mismatch
    BatchIdMismatch = 6010,
    /// Pool mismatch
    PoolMismatch = 6011,
    /// Owner mismatch
    OwnerMismatch = 6012,
    /// Clearing price computation failed
    ClearingPriceFailed = 6013,
    /// Window slots must be > 0
    InvalidWindowSlots = 6014,
    /// Account already initialized
    AlreadyInitialized = 6015,
    /// Caller is not the pool authority
    Unauthorized = 6016,
    /// Oracle feed account not owned by Switchboard program
    OracleOwnerMismatch = 6017,
    /// Pool is paused
    PoolPaused = 6018,
    /// InitializePool base_fee_bps ≥ 10_000 (fee ≥ 100%)
    InvalidFeeBps = 6019,
    /// Vault account does not match pool.vault_a / pool.vault_b
    VaultMismatch = 6020,
    /// Jito bid is below MIN_BID_LAMPORTS
    BidTooLow = 6021,
    /// next_window_end would overflow u64
    WindowEndOverflow = 6022,
    /// ClearBatch bid recipient does not match `pool.authority` (#59).
    /// pfda-amm doesn't carry a separate `treasury` field today, so the
    /// pool authority is the canonical bid recipient until a dedicated
    /// treasury schema is added in a follow-up.
    TreasuryMismatch = 6023,
    /// ClearBatch received a non-zero bid but no treasury/authority
    /// account was supplied to receive it (#59). Without the account,
    /// the Transfer CPI target is undefined — reject before any work.
    BidWithoutTreasury = 6024,
}

impl From<PfmmError> for ProgramError {
    fn from(e: PfmmError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
