use litesvm::LiteSVM;
use solana_account::Account;
use solana_address::Address;
use solana_native_token::LAMPORTS_PER_SOL;

pub const TOKEN_PROGRAM_ID_BYTES: [u8; 32] = [
    0x06, 0xdd, 0xf6, 0xe1, 0xd7, 0x65, 0xa1, 0x93, 0xd9, 0xcb, 0xe1, 0x46, 0xce, 0xeb, 0x79, 0xac,
    0x1c, 0xb4, 0x85, 0xed, 0x5f, 0x5b, 0x37, 0x91, 0x3a, 0x8c, 0xf5, 0x85, 0x7e, 0xff, 0x00, 0xa9,
];

pub fn token_program_id() -> Address {
    Address::from(TOKEN_PROGRAM_ID_BYTES)
}

pub fn system_program_id() -> Address {
    Address::from([0u8; 32])
}

/// Create a synthetic SPL token mint (82 bytes).
pub fn create_mint(svm: &mut LiteSVM, mint_addr: Address, authority: &Address, decimals: u8) {
    let mut data = vec![0u8; 82];
    // COption::Some(authority)
    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    data[4..36].copy_from_slice(authority.as_ref());
    // supply = 0 (already zeroed)
    // decimals
    data[44] = decimals;
    // is_initialized = true
    data[45] = 1;

    svm.set_account(
        mint_addr,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data,
            owner: token_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

/// Create a synthetic SPL token account (165 bytes).
pub fn create_token_account(
    svm: &mut LiteSVM,
    addr: Address,
    mint: &Address,
    owner: &Address,
    amount: u64,
) {
    let mut data = vec![0u8; 165];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // state = Initialized (1)
    data[108] = 1;

    svm.set_account(
        addr,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data,
            owner: token_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

/// Create an uninitialized token account (for PFDA-3 InitializePool to call InitializeAccount3).
pub fn create_uninit_token_account(svm: &mut LiteSVM, addr: Address) {
    svm.set_account(
        addr,
        Account {
            lamports: LAMPORTS_PER_SOL,
            data: vec![0u8; 165],
            owner: token_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

/// Create a token account with extra data padding for CPI realloc headroom.
/// Used for vault accounts that Jupiter may need to resize during swaps.
pub fn create_token_account_padded(
    svm: &mut LiteSVM,
    addr: Address,
    mint: &Address,
    owner: &Address,
    amount: u64,
    extra_bytes: usize,
) {
    let total_size = 165 + extra_bytes;
    let mut data = vec![0u8; total_size];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    data[108] = 1; // Initialized

    svm.set_account(
        addr,
        Account {
            lamports: LAMPORTS_PER_SOL * 2, // extra SOL for rent on larger account
            data,
            owner: token_program_id(),
            executable: false,
            rent_epoch: 0,
        },
    )
    .unwrap();
}

/// Read token amount from an SPL token account (offset 64, 8 bytes LE).
pub fn read_token_amount(svm: &LiteSVM, addr: &Address) -> u64 {
    let acc = svm.get_account(addr).expect("account not found");
    u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
}
