//! Metaplex Token Metadata CPI helpers for axis-vault CreateEtf (v1.1).
//!
//! Provides:
//!   - `METAPLEX_TOKEN_METADATA_PROGRAM_ID` for CPI validation
//!   - `MAX_NAME_LENGTH`, `MAX_SYMBOL_LENGTH`, `MAX_URI_LENGTH` per
//!     Metaplex DataV2 spec (mpl-token-metadata v1.13.x)
//!   - `invoke_create_metadata_v3` — builds + invokes
//!     `CreateMetadataAccountV3` with etfState PDA signing as
//!     mint_authority (and update_authority pinned to etfState).
//!
//! No `mpl-token-metadata` or `borsh` crate dep — the borsh layout is
//! hand-rolled to keep Pinocchio's no-heap surface clean. Mirrors the
//! `jupiter.rs` pattern for arbitrary-CPI defence (program ID pinned,
//! metas built by hand).

use pinocchio::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction, Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::error::VaultError;

/// Metaplex Token Metadata Program ID bytes (base58
/// `metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s`). Pinned constant —
/// CreateEtf rejects any other key in the metaplex_program slot before
/// CPI to close the arbitrary-CPI vector. Mirrors
/// `jupiter::JUPITER_PROGRAM_ID`.
pub const METAPLEX_TOKEN_METADATA_PROGRAM_ID: [u8; 32] = [
    0x0b, 0x70, 0x65, 0xb1, 0xe3, 0xd1, 0x7c, 0x45,
    0x38, 0x9d, 0x52, 0x7f, 0x6b, 0x04, 0xc3, 0xcd,
    0x58, 0xb8, 0x6c, 0x73, 0x1a, 0xa0, 0xfd, 0xb5,
    0x49, 0xb6, 0xd1, 0xbc, 0x03, 0xf8, 0x29, 0x46,
];

/// CreateMetadataAccountV3 instruction discriminator.
pub const CREATE_METADATA_ACCOUNT_V3_DISC: u8 = 33;

/// DataV2.name length cap per Metaplex.
pub const MAX_NAME_LENGTH: usize = 32;
/// DataV2.symbol length cap per Metaplex. Tighter than our 16-byte
/// state slot; CreateEtf re-validates ticker against this before CPI.
pub const MAX_SYMBOL_LENGTH: usize = 10;
/// DataV2.uri length cap per Metaplex.
pub const MAX_URI_LENGTH: usize = 200;

/// Worst-case borsh payload size:
///   1  (disc)
/// + 4 + MAX_NAME_LENGTH      = 36 (name string)
/// + 4 + MAX_SYMBOL_LENGTH    = 14 (symbol string)
/// + 4 + MAX_URI_LENGTH       = 204 (uri string)
/// + 2  (seller_fee_basis_points)
/// + 1 + 1 + 1                = 3 (creators=None, collection=None, uses=None)
/// + 1  (is_mutable)
/// + 1  (collection_details=None)
/// = 262
const METAPLEX_CPI_MAX_DATA: usize = 262;

/// CreateMetadataAccountV3 takes 6 accounts (rent sysvar omitted —
/// optional in v1.13+ and the runtime supplies it via syscall).
const METAPLEX_CPI_ACCOUNTS: usize = 6;

/// Build + invoke `CreateMetadataAccountV3` against the deployed
/// Metaplex Token Metadata program.
///
/// Authority model (matches AXIS_VAULT_V1_1_SPEC §3):
///   - mint_authority   = etfState PDA, signed via `etf_state_seeds`
///   - update_authority = etfState PDA (pinned, non-signer at create —
///                        future v1.2 UpdateEtfMetadata re-derives
///                        seeds and signs)
///   - is_mutable       = true
///   - seller_fee_basis_points = 0 (fungible)
///   - creators / collection / uses = None
///
/// Validates:
///   - `metaplex_program.key() == METAPLEX_TOKEN_METADATA_PROGRAM_ID`
///   - `name.len() <= MAX_NAME_LENGTH`
///   - `symbol.len() <= MAX_SYMBOL_LENGTH`
///   - `uri.len()  <= MAX_URI_LENGTH`
///
/// Caller MUST verify `metadata_pda` derives from
/// `[b"metadata", METAPLEX_TOKEN_METADATA_PROGRAM_ID, etf_mint]` (the
/// PDA derivation is left to the caller because find_program_address
/// is ~10k CU and the caller usually already needs the bump for
/// other reasons).
#[allow(clippy::too_many_arguments)]
pub fn invoke_create_metadata_v3(
    metaplex_program: &AccountInfo,
    metadata_pda: &AccountInfo,
    etf_mint: &AccountInfo,
    etf_state_pda: &AccountInfo,
    payer: &AccountInfo,
    system_program: &AccountInfo,
    name: &[u8],
    symbol: &[u8],
    uri: &[u8],
    etf_state_seeds: &[Seed],
) -> Result<(), ProgramError> {
    if metaplex_program.key().as_ref() != &METAPLEX_TOKEN_METADATA_PROGRAM_ID {
        return Err(VaultError::InvalidMetaplexProgram.into());
    }
    if name.len() > MAX_NAME_LENGTH {
        return Err(VaultError::InvalidName.into());
    }
    if symbol.len() > MAX_SYMBOL_LENGTH {
        return Err(VaultError::InvalidTicker.into());
    }
    if uri.len() > MAX_URI_LENGTH {
        return Err(VaultError::InvalidUri.into());
    }

    // Borsh-encode CreateMetadataAccountV3InstructionArgs onto the
    // stack. Layout matches mpl-token-metadata v1.13.x.
    let mut buf = [0u8; METAPLEX_CPI_MAX_DATA];
    let mut pos = 0usize;
    buf[pos] = CREATE_METADATA_ACCOUNT_V3_DISC;
    pos += 1;
    pos = write_borsh_string(&mut buf, pos, name);
    pos = write_borsh_string(&mut buf, pos, symbol);
    pos = write_borsh_string(&mut buf, pos, uri);
    // seller_fee_basis_points: u16 LE = 0 (fungible, no NFT royalty).
    buf[pos] = 0;
    buf[pos + 1] = 0;
    pos += 2;
    // creators: Option<Vec<Creator>> = None.
    buf[pos] = 0;
    pos += 1;
    // collection: Option<Collection> = None.
    buf[pos] = 0;
    pos += 1;
    // uses: Option<Uses> = None.
    buf[pos] = 0;
    pos += 1;
    // is_mutable: bool = true. Lets v1.2 `UpdateEtfMetadata` ship
    // without a discriminator bump or new mint.
    buf[pos] = 1;
    pos += 1;
    // collection_details: Option<CollectionDetails> = None.
    buf[pos] = 0;
    pos += 1;

    // AccountMeta order must match Metaplex's account list:
    //   0: metadata         [writable, non-signer]
    //   1: mint             [readonly, non-signer]
    //   2: mint_authority   [readonly, signer (PDA-signed via seeds)]
    //   3: payer            [writable, signer]
    //   4: update_authority [readonly, non-signer]
    //   5: system_program   [readonly, non-signer]
    let metaplex_pid = unsafe {
        &*(&METAPLEX_TOKEN_METADATA_PROGRAM_ID as *const [u8; 32] as *const Pubkey)
    };
    let metas: [AccountMeta; METAPLEX_CPI_ACCOUNTS] = [
        AccountMeta::writable(metadata_pda.key()),
        AccountMeta::readonly(etf_mint.key()),
        AccountMeta::readonly_signer(etf_state_pda.key()),
        AccountMeta::writable_signer(payer.key()),
        AccountMeta::readonly(etf_state_pda.key()),
        AccountMeta::readonly(system_program.key()),
    ];

    let cpi_ix = Instruction {
        program_id: metaplex_pid,
        accounts: &metas,
        data: &buf[..pos],
    };

    let signer = Signer::from(etf_state_seeds);
    let signers = [signer];
    let account_infos: [&AccountInfo; METAPLEX_CPI_ACCOUNTS] = [
        metadata_pda,
        etf_mint,
        etf_state_pda,
        payer,
        etf_state_pda,
        system_program,
    ];
    pinocchio::cpi::invoke_signed_with_bounds::<METAPLEX_CPI_ACCOUNTS>(
        &cpi_ix,
        &account_infos,
        &signers,
    )
}

/// Writes a borsh-encoded String (`[u32 LE len][bytes]`) into `buf`
/// starting at `pos`. Returns the new cursor position.
#[inline(always)]
fn write_borsh_string(buf: &mut [u8], pos: usize, s: &[u8]) -> usize {
    let len = s.len() as u32;
    buf[pos..pos + 4].copy_from_slice(&len.to_le_bytes());
    let after_len = pos + 4;
    buf[after_len..after_len + s.len()].copy_from_slice(s);
    after_len + s.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-checked layout for a small payload. Bytes must match
    /// exactly what mpl-token-metadata v1.13.x's BorshSerialize
    /// produces for `CreateMetadataAccountV3InstructionArgs`.
    #[test]
    fn borsh_layout_minimal() {
        let mut buf = [0u8; METAPLEX_CPI_MAX_DATA];
        let mut pos = 0;
        buf[pos] = CREATE_METADATA_ACCOUNT_V3_DISC;
        pos += 1;
        pos = write_borsh_string(&mut buf, pos, b"AXIS");
        pos = write_borsh_string(&mut buf, pos, b"AXIS");
        pos = write_borsh_string(&mut buf, pos, b"https://axis.io/etf/test.json");
        buf[pos] = 0; buf[pos + 1] = 0; pos += 2; // sfbp
        buf[pos] = 0; pos += 1;                   // creators=None
        buf[pos] = 0; pos += 1;                   // collection=None
        buf[pos] = 0; pos += 1;                   // uses=None
        buf[pos] = 1; pos += 1;                   // is_mutable=true
        buf[pos] = 0; pos += 1;                   // collection_details=None

        assert_eq!(buf[0], 33);
        assert_eq!(&buf[1..5], &[4u8, 0, 0, 0]);
        assert_eq!(&buf[5..9], b"AXIS");
        assert_eq!(&buf[9..13], &[4u8, 0, 0, 0]);
        assert_eq!(&buf[13..17], b"AXIS");
        assert_eq!(&buf[17..21], &[29u8, 0, 0, 0]);
        assert_eq!(&buf[21..50], b"https://axis.io/etf/test.json");
        assert_eq!(&buf[50..52], &[0u8, 0]);
        assert_eq!(buf[52], 0);
        assert_eq!(buf[53], 0);
        assert_eq!(buf[54], 0);
        assert_eq!(buf[55], 1);
        assert_eq!(buf[56], 0);
        assert_eq!(pos, 57);
    }

    #[test]
    fn empty_uri_packs_with_zero_len_prefix() {
        let mut buf = [0u8; METAPLEX_CPI_MAX_DATA];
        let pos = write_borsh_string(&mut buf, 0, b"");
        assert_eq!(pos, 4);
        assert_eq!(&buf[0..4], &[0u8, 0, 0, 0]);
    }

    #[test]
    fn worst_case_payload_fits_in_buffer() {
        // 1 + 4+32 + 4+10 + 4+200 + 2 + 3 + 1 + 1 = 262
        let max = 1 + (4 + MAX_NAME_LENGTH) + (4 + MAX_SYMBOL_LENGTH)
            + (4 + MAX_URI_LENGTH) + 2 + 3 + 1 + 1;
        assert_eq!(max, METAPLEX_CPI_MAX_DATA);
    }
}
