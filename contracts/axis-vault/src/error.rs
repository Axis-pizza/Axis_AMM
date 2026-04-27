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
    /// Pre-implementation placeholder from PR #58. Retained so older
    /// clients receive a recognisable code if they hit a stale build,
    /// but unused in the current implementation. Issue #36.
    NotYetImplemented = 9024,
    /// DepositSol / WithdrawSol restrict basket size to keep CU + tx
    /// account count within the 1.4M / versioned-tx envelopes. 5-leg
    /// baskets must use the client-bundled flow from PR #42. Issue #36.
    BasketTooLargeForOnchainSol = 9025,
    /// DepositSol / WithdrawSol passed a `jupiter_program` account that
    /// does not match the canonical Jupiter V6 program ID. Closes the
    /// arbitrary-CPI vector — without this check a malicious
    /// `jupiter_program` could simulate the swap and pocket the wSOL.
    InvalidJupiterProgram = 9026,
    /// DepositSol / WithdrawSol passed a `wsol_mint` that does not
    /// match the canonical wrapped-SOL mint. Lets us refuse to wrap
    /// into a substitute mint that Jupiter would happily route.
    WsolMintMismatch = 9027,
    /// DepositSol per-leg `leg_sol_amount` values do not sum to the
    /// `sol_in` total declared in the instruction header. Catches a
    /// mis-built tx before any wSOL is wrapped.
    LegSumMismatch = 9028,
    /// DepositSol / WithdrawSol `leg_count` does not equal the ETF's
    /// stored `token_count`. The on-chain SOL ixes require the basket
    /// to be fully covered — partial baskets are not supported.
    LegCountMismatch = 9029,
    /// DepositSol Jupiter CPI returned without depositing any tokens
    /// into one of the basket vaults — typically Jupiter aborted the
    /// route silently, or the `route_bytes` were stale and the slippage
    /// check on Jupiter's side fired. We refuse to mint against a
    /// no-op leg.
    JupiterCpiNoOutput = 9030,
    /// First DepositSol on an ETF whose `total_supply` is still zero.
    /// Bootstrapping requires basket-token Deposit so the seed
    /// composition matches target weights — DepositSol can only be
    /// used after the ETF has been seeded.
    EtfNotBootstrapped = 9031,
    /// Per-leg payload (leg_sol_amount + route_len + route_bytes) was
    /// truncated, malformed, or went past the instruction-data tail.
    MalformedLegData = 9032,
    /// SetFee rejected: requested `new_fee_bps` exceeds the per-ETF
    /// `max_fee_bps` ceiling captured at CreateEtf time, or exceeds
    /// the program-wide `MAX_FEE_BPS_CEILING`. The hard ceiling
    /// protects users from a compromised authority key dialling fees
    /// up to 100 % and draining deposits.
    FeeTooHigh = 9033,
    /// Deposit / DepositSol rejected: the resulting `total_supply`
    /// would exceed `tvl_cap`. Closed-beta ramp gate. Either wait for
    /// the authority to raise the cap (SetCap) or deposit a smaller
    /// amount.
    TvlCapExceeded = 9034,
    /// SetCap rejected: requested `new_cap` is below the current cap.
    /// Lowering the cap would strand any pool currently above it
    /// (deposits revert with TvlCapExceeded but withdrawals pay the
    /// per-vault share computed against current state — there's no
    /// in-protocol drain path back to the lower cap). The cap is
    /// monotonically increasing.
    InvalidCapDecrease = 9035,
    /// WithdrawSol rejected: the per-leg Jupiter CPI consumed more
    /// from `vault[i]` than the burn-share `per_vault_amount[i]`
    /// allows. Defends against a malicious `route_bytes.inAmount`
    /// being used (with the program-signed vault PDA) to drain
    /// beyond the user's pro-rata claim. Bound is checked post-CPI
    /// on each vault's input-side delta.
    ExcessVaultDrain = 9036,
}

impl From<VaultError> for ProgramError {
    fn from(e: VaultError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
