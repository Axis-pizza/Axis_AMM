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
}

impl From<VaultError> for ProgramError {
    fn from(e: VaultError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
