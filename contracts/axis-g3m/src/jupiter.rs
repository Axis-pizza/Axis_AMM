/// Jupiter integration for axis-g3m rebalancing.
///
/// Provides:
///   1. `JUPITER_PROGRAM_ID` constant for validation.
///   2. `read_vault_balance()` — SPL token account balance reader.
///   3. `process_rebalance_via_jupiter()` — CPI-backed rebalance with
///      post-swap vault verification and invariant check.
///
/// Jupiter V6 program ID: JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4

use pinocchio::{
    account_info::AccountInfo,
    instruction::{AccountMeta, Instruction, Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::{clock::Clock, Sysvar},
    ProgramResult,
};

use crate::error::G3mError;
use crate::math::compute_invariant;
use crate::state::G3mPoolState;

/// Validate that a program account is the Jupiter V6 program.
pub fn verify_jupiter_program(program_account: &AccountInfo) -> Result<(), ProgramError> {
    if program_account.key().as_ref() != &JUPITER_PROGRAM_ID {
        return Err(G3mError::InvalidProgram.into());
    }
    Ok(())
}

/// Jupiter V6 program ID bytes
pub const JUPITER_PROGRAM_ID: [u8; 32] = [
    0x04, 0x58, 0x99, 0x26, 0x88, 0x1e, 0xd7, 0x10,
    0x0e, 0x13, 0x14, 0x20, 0x8b, 0x1e, 0x54, 0x25,
    0x42, 0x79, 0x4f, 0x5d, 0x08, 0x16, 0x31, 0x76,
    0xa0, 0x73, 0xb2, 0x79, 0x81, 0x40, 0x2e, 0x72,
];

/// Read token account balance from SPL token account data.
/// SPL token account layout: amount is at offset 64, 8 bytes LE.
pub fn read_vault_balance(account: &AccountInfo) -> Result<u64, ProgramError> {
    let data = account.try_borrow_data()?;
    if data.len() < 72 {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(u64::from_le_bytes(
        data[64..72]
            .try_into()
            .map_err(|_| ProgramError::InvalidAccountData)?,
    ))
}

/// Maximum number of accounts for a Jupiter CPI (vaults + route accounts).
const MAX_JUPITER_CPI_ACCOUNTS: usize = 32;

/// RebalanceViaJupiter — execute a Jupiter swap and verify post-swap invariant.
///
/// This replaces the attestation-mode rebalance with a trustless CPI flow:
///   1. Verify drift threshold exceeded + cooldown elapsed
///   2. Snapshot pre-swap vault balances and invariant
///   3. Invoke Jupiter V6 swap via CPI (pool PDA as signer)
///   4. Read post-swap vault balances
///   5. Verify invariant is maintained (1% tolerance)
///   6. Verify per-token weight drift is within threshold
///
/// Accounts:
///   0:  authority         (signer, must be pool authority)
///   1:  pool_state        (writable, PDA)
///   2:  jupiter_program   (executable, must match JUPITER_PROGRAM_ID)
///   3..3+N: vault accounts (writable, pool token vaults)
///   3+N..: remaining Jupiter route accounts (passed through to CPI)
///
/// Instruction data (after 1-byte discriminant):
///   [0..4]:  jupiter_data_len: u32 LE
///   [4..4+jupiter_data_len]: Jupiter swap instruction data (opaque)
pub fn process_rebalance_via_jupiter(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    jupiter_data: &[u8],
) -> ProgramResult {
    if accounts.len() < 3 {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    let authority = &accounts[0];
    let pool_account = &accounts[1];
    let jupiter_program = &accounts[2];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Validate Jupiter program ID
    if jupiter_program.key().as_ref() != &JUPITER_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Load pool state (immutable first pass)
    let pool_data = pool_account.try_borrow_data()?;
    let pool = unsafe { &*(pool_data.as_ptr() as *const G3mPoolState) };

    if !pool.is_initialized() {
        return Err(G3mError::InvalidDiscriminator.into());
    }
    if pool.paused != 0 {
        return Err(G3mError::PoolPaused.into());
    }
    if authority.key().as_ref() != &pool.authority {
        return Err(G3mError::OwnerMismatch.into());
    }

    let tc = pool.token_count as usize;

    // Enforce cooldown
    let current_slot = Clock::get()?.slot;
    let slots_since = current_slot.saturating_sub(pool.last_rebalance_slot);
    if slots_since < pool.rebalance_cooldown {
        return Err(G3mError::CooldownActive.into());
    }

    // Verify drift threshold is exceeded
    if !pool.needs_rebalance() {
        return Err(G3mError::DriftBelowThreshold.into());
    }

    // Require vault accounts: accounts[3..3+tc]
    if accounts.len() < 3 + tc {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // Snapshot pre-swap vault balances and compute fresh invariant
    let mut pre_balances = [0u64; 5];
    for i in 0..tc {
        let vault = &accounts[3 + i];
        if vault.key().as_ref() != &pool.token_vaults[i] {
            return Err(G3mError::PoolMismatch.into());
        }
        // Full vault verification: owner = token program, mint + owner match pool
        crate::security::verify_vault(
            vault,
            &pool.token_mints[i],
            pool_account.key().as_ref().try_into().unwrap(),
        )?;
        pre_balances[i] = read_vault_balance(vault)?;
    }

    // Compute fresh invariant from current vault balances (not stale stored k)
    let old_k = compute_invariant(
        &pre_balances,
        &pool.target_weights_bps,
        tc,
    ).ok_or(ProgramError::from(G3mError::Overflow))?;

    let authority_bytes = pool.authority;
    let bump_bytes = [pool.bump];
    drop(pool_data);

    // === Execute Jupiter CPI ===
    // Build the CPI instruction with pool PDA as signer authority.
    // The remaining accounts (after 3+tc) are Jupiter route accounts.
    let jupiter_accounts_start = 3 + tc;
    let cpi_account_count = accounts.len() - jupiter_accounts_start + tc;

    if cpi_account_count > MAX_JUPITER_CPI_ACCOUNTS {
        return Err(ProgramError::InvalidArgument);
    }

    // Build AccountMeta list: vault accounts (writable) + remaining route accounts.
    // Use a Vec-like pattern with MaybeUninit to avoid UB from zero-initializing refs.
    let mut cpi_metas_storage: [core::mem::MaybeUninit<AccountMeta>; MAX_JUPITER_CPI_ACCOUNTS] =
        unsafe { core::mem::MaybeUninit::uninit().assume_init() };
    let mut meta_count = 0;

    for i in 0..tc {
        cpi_metas_storage[meta_count].write(AccountMeta::writable(accounts[3 + i].key()));
        meta_count += 1;
    }
    for i in jupiter_accounts_start..accounts.len() {
        if meta_count >= MAX_JUPITER_CPI_ACCOUNTS {
            break;
        }
        cpi_metas_storage[meta_count].write(AccountMeta::from(&accounts[i]));
        meta_count += 1;
    }

    // SAFETY: cpi_metas_storage[..meta_count] elements have been initialized above.
    let cpi_metas: &[AccountMeta] = unsafe {
        core::slice::from_raw_parts(
            cpi_metas_storage.as_ptr() as *const AccountMeta,
            meta_count,
        )
    };

    let jup_pid = unsafe { &*(&JUPITER_PROGRAM_ID as *const [u8; 32] as *const Pubkey) };
    let cpi_instruction = Instruction {
        program_id: jup_pid,
        accounts: cpi_metas,
        data: jupiter_data,
    };

    // Collect account info references for CPI
    let mut cpi_infos_storage: [core::mem::MaybeUninit<&AccountInfo>; MAX_JUPITER_CPI_ACCOUNTS] =
        unsafe { core::mem::MaybeUninit::uninit().assume_init() };
    let mut info_count = 0;

    for i in 0..tc {
        cpi_infos_storage[info_count].write(&accounts[3 + i]);
        info_count += 1;
    }
    for i in jupiter_accounts_start..accounts.len() {
        if info_count >= MAX_JUPITER_CPI_ACCOUNTS {
            break;
        }
        cpi_infos_storage[info_count].write(&accounts[i]);
        info_count += 1;
    }

    // SAFETY: cpi_infos_storage[..info_count] elements have been initialized above.
    let cpi_infos: &[&AccountInfo] = unsafe {
        core::slice::from_raw_parts(
            cpi_infos_storage.as_ptr() as *const &AccountInfo,
            info_count,
        )
    };

    // Pool PDA signs the CPI
    let pool_signer_seeds = [
        Seed::from(b"g3m_pool".as_slice()),
        Seed::from(authority_bytes.as_slice()),
        Seed::from(&bump_bytes),
    ];
    let pool_signer = [Signer::from(&pool_signer_seeds)];

    pinocchio::cpi::invoke_signed_with_bounds::<MAX_JUPITER_CPI_ACCOUNTS>(
        &cpi_instruction,
        cpi_infos,
        &pool_signer,
    )?;

    // === Post-swap verification ===
    // Read new vault balances and verify invariant + per-token weights.
    let mut post_balances = [0u64; 5];
    for i in 0..tc {
        post_balances[i] = read_vault_balance(&accounts[3 + i])?;
    }

    // Update pool state with actual vault balances
    let data = pool_account.try_borrow_mut_data()?;
    let pool = unsafe { &mut *(data.as_ptr() as *mut G3mPoolState) };

    for i in 0..tc {
        pool.reserves[i] = post_balances[i];
    }

    // Verify invariant (1% tolerance — trustless mode, ground truth from vaults)
    let new_k = compute_invariant(
        &pool.reserves,
        &pool.target_weights_bps,
        tc,
    ).ok_or(ProgramError::from(G3mError::Overflow))?;

    let min_k = old_k
        .checked_mul(99)
        .ok_or(ProgramError::from(G3mError::Overflow))?
        .checked_div(100)
        .ok_or(ProgramError::from(G3mError::DivisionByZero))?;

    if new_k < min_k {
        return Err(G3mError::InvariantViolation.into());
    }

    // Verify per-token weight drift is within threshold
    for i in 0..tc {
        let target = pool.target_weights_bps[i] as u64;
        if target == 0 {
            continue;
        }
        let actual = pool.actual_weight_bps(i)
            .ok_or(ProgramError::from(G3mError::Overflow))?;
        let diff = if actual > target {
            actual - target
        } else {
            target - actual
        };
        let post_drift = diff
            .checked_mul(10_000)
            .ok_or(ProgramError::from(G3mError::Overflow))?
            .checked_div(target)
            .ok_or(ProgramError::from(G3mError::DivisionByZero))?;
        if post_drift > pool.drift_threshold_bps as u64 {
            return Err(G3mError::PerTokenDriftExceeded.into());
        }
    }

    pool.set_invariant_k(new_k);
    pool.last_rebalance_slot = current_slot;

    // Emit metrics (32 bytes: old_k + new_k)
    let mut return_buf = [0u8; 32];
    return_buf[0..16].copy_from_slice(&old_k.to_le_bytes());
    return_buf[16..32].copy_from_slice(&new_k.to_le_bytes());
    pinocchio::program::set_return_data(&return_buf);

    Ok(())
}
