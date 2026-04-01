use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::Pubkey,
    ProgramResult,
};
use pinocchio_token::instructions::Transfer;

use crate::error::G3mError;
use crate::math::{compute_invariant, compute_swap_output};
use crate::state::G3mPoolState;

/// Swap — execute a swap between two tokens in the G3M pool.
///
/// Accounts:
///   0: user            (signer)
///   1: pool_state      (writable, PDA)
///   2: user_token_in   (writable, user's source token account)
///   3: user_token_out  (writable, user's destination token account)
///   4: vault_in        (writable, pool's vault for input token)
///   5: vault_out       (writable, pool's vault for output token)
///   6: token_program
///
/// Instruction data (after 1-byte discriminant):
///   [0]:    in_token_index: u8
///   [1]:    out_token_index: u8
///   [2..10]:  amount_in: u64 LE
///   [10..18]: min_amount_out: u64 LE
pub fn process_swap(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    in_idx: u8,
    out_idx: u8,
    amount_in: u64,
    min_amount_out: u64,
) -> ProgramResult {
    let user = &accounts[0];
    let pool_account = &accounts[1];
    let user_token_in = &accounts[2];
    let user_token_out = &accounts[3];
    let vault_in = &accounts[4];
    let vault_out = &accounts[5];
    let _token_program = &accounts[6];

    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Read pool state
    let pool_data = pool_account.try_borrow_data()?;
    let pool = unsafe { &*(pool_data.as_ptr() as *const G3mPoolState) };

    if !pool.is_initialized() {
        return Err(G3mError::InvalidDiscriminator.into());
    }
    if pool.paused != 0 {
        return Err(G3mError::PoolPaused.into());
    }
    if amount_in == 0 {
        return Err(G3mError::ZeroAmount.into());
    }

    let tc = pool.token_count as usize;
    let in_i = in_idx as usize;
    let out_i = out_idx as usize;

    if in_i >= tc || out_i >= tc || in_i == out_i {
        return Err(G3mError::InvalidTokenIndex.into());
    }

    // Validate vault accounts match pool state + security checks
    if vault_in.key().as_ref() != &pool.token_vaults[in_i] {
        return Err(G3mError::PoolMismatch.into());
    }
    if vault_out.key().as_ref() != &pool.token_vaults[out_i] {
        return Err(G3mError::PoolMismatch.into());
    }
    // Security rule 6: verify token accounts owned by token program
    crate::security::verify_token_account_owner(vault_in)?;
    crate::security::verify_token_account_owner(vault_out)?;
    crate::security::verify_token_account_owner(user_token_in)?;
    crate::security::verify_token_account_owner(user_token_out)?;

    // Compute swap output
    let amount_out = compute_swap_output(
        &pool.reserves,
        &pool.target_weights_bps,
        tc,
        in_i,
        out_i,
        amount_in,
        pool.fee_rate_bps,
    )
    .ok_or(ProgramError::from(G3mError::Overflow))?;

    if amount_out < min_amount_out {
        return Err(G3mError::SlippageExceeded.into());
    }
    if amount_out > pool.reserves[out_i] {
        return Err(G3mError::Overflow.into());
    }

    drop(pool_data);

    // Transfer in: user -> vault
    Transfer {
        from: user_token_in,
        to: vault_in,
        authority: user,
        amount: amount_in,
    }
    .invoke()?;

    // Transfer out: vault -> user (PDA signer)
    let pool_data = pool_account.try_borrow_data()?;
    let pool = unsafe { &*(pool_data.as_ptr() as *const G3mPoolState) };
    let bump_bytes = [pool.bump];
    let authority_bytes = pool.authority;
    drop(pool_data);

    let signer_seeds = [
        Seed::from(b"g3m_pool".as_slice()),
        Seed::from(authority_bytes.as_slice()),
        Seed::from(&bump_bytes),
    ];
    let signers = [Signer::from(&signer_seeds)];

    Transfer {
        from: vault_out,
        to: user_token_out,
        authority: pool_account,
        amount: amount_out,
    }
    .invoke_signed(&signers)?;

    // Update pool reserves
    let data = pool_account.try_borrow_mut_data()?;
    let pool = unsafe { &mut *(data.as_ptr() as *mut G3mPoolState) };

    pool.reserves[in_i] = pool.reserves[in_i]
        .checked_add(amount_in)
        .ok_or(ProgramError::from(G3mError::Overflow))?;
    pool.reserves[out_i] = pool.reserves[out_i]
        .checked_sub(amount_out)
        .ok_or(ProgramError::from(G3mError::Overflow))?;

    // Verify invariant is maintained or increased
    let new_k = compute_invariant(
        &pool.reserves,
        &pool.target_weights_bps,
        tc,
    )
    .ok_or(ProgramError::from(G3mError::Overflow))?;

    if new_k < pool.invariant_k() {
        return Err(G3mError::InvariantViolation.into());
    }
    pool.set_invariant_k(new_k);

    Ok(())
}
