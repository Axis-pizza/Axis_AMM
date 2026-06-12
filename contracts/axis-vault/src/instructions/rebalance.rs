//! Rebalance — authority-gated single-pair basket rebalance via Jupiter.
//!
//! Swaps `amount_in` of basket token `sell_index` into basket token
//! `buy_index` through one Jupiter V6 route, signed by the etf_state
//! PDA (same signing pattern as WithdrawSol legs). One swap per
//! transaction: a multi-leg route cannot fit the 1232-byte tx envelope
//! (DepositSol already needs a maxAccounts ladder for a single leg), so
//! multi-asset rebalances are several Rebalance transactions sharing
//! one turnover window.
//!
//! What the chain enforces (no oracle, so no execution-price check):
//!   - caller is `etf_state.authority` and the ETF is not paused
//!   - per-window, per-vault turnover cap (`MAX_TURNOVER_BPS` against
//!     the window-open snapshot)
//!   - sell vault loses at most `amount_in`
//!   - buy vault gains at least `min_out` (> 0 — output must reach
//!     custody, not an attacker ATA)
//!   - every other vault balance is non-decreasing
//!
//! Rebalance never touches `total_supply` or the share mint — it only
//! changes basket composition, so share accounting is unaffected.
//!
//! # Account layout
//!
//! ```text
//! 0:  [signer, writable]  authority (pays sidecar rent on first use)
//! 1:  []                  etf_state PDA
//! 2:  [writable]          rebalance_state PDA ([b"rebal", etf_state])
//! 3:  []                  system_program
//! 4..4+tc:   [writable]   basket vaults (order = etf.token_vaults)
//! 4+tc:      []           jupiter_program
//! 4+tc+1..:  []           Jupiter route accounts (route_account_count)
//! ```
//!
//! # Instruction data (after the discriminator byte)
//!
//! ```text
//! [sell_index: u8][buy_index: u8]
//! [amount_in: u64 LE][min_out: u64 LE]
//! [route_account_count: u8]
//! [route_len: u32 LE][route_bytes: route_len bytes]
//! ```
//!
//! Per-leg CPI metas are `[sell_vault] + route_accounts`, the same
//! convention WithdrawSol uses for its vault-side legs.

use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    pubkey::{self, Pubkey},
    sysvars::{clock::Clock, rent::Rent, Sysvar},
    ProgramResult,
};
use pinocchio_system::instructions::{Allocate, Assign, CreateAccount, Transfer};

use crate::constants::{MAX_TURNOVER_BPS, REBALANCE_WINDOW_SLOTS, TOKEN_PROGRAM_ID};
use crate::error::VaultError;
use crate::jupiter::{
    invoke_jupiter_leg, read_token_account_balance, JUPITER_PROGRAM_ID, MAX_JUPITER_CPI_ACCOUNTS,
};
use crate::state::{load, load_mut, EtfState, RebalanceState, MAX_BASKET_TOKENS};

const FIXED_ACCOUNTS: usize = 4;

/// Validate the rebalance sidecar PDA for `etf_state_key`, creating and
/// initializing it on first use (payer funds the rent). On an existing
/// account, checks owner, discriminator and the stored back-pointer.
pub(crate) fn ensure_rebalance_state(
    program_id: &Pubkey,
    payer: &AccountInfo,
    rebalance_state_ai: &AccountInfo,
    etf_state_key: &Pubkey,
) -> Result<(), ProgramError> {
    let (expected, bump) =
        pubkey::find_program_address(&[b"rebal", etf_state_key], program_id);
    if rebalance_state_ai.key() != &expected {
        return Err(ProgramError::InvalidSeeds);
    }

    if rebalance_state_ai.owner() == program_id {
        let data = rebalance_state_ai.try_borrow_data()?;
        let st = unsafe { load::<RebalanceState>(&data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        if !st.is_initialized() || &st.etf_state != etf_state_key {
            return Err(VaultError::InvalidRebalanceState.into());
        }
        return Ok(());
    }

    // First use — create + initialize. The sidecar address is a
    // deterministic function of the (public) etf_state key, so anyone
    // can compute it and `System::Transfer` a lamport to it ahead of
    // time. A plain `CreateAccount` aborts with AccountAlreadyInUse on
    // a pre-funded address, which would let a griefer permanently brick
    // Rebalance + weight governance for a targeted ETF. Adopt the
    // pre-funded case instead: top up to rent-exemption, allocate, and
    // assign to this program — all under the PDA signature, which the
    // griefer cannot forge, so they can only ever *donate* rent.
    let rent = Rent::get()?;
    let needed = rent.minimum_balance(RebalanceState::LEN);
    let space = RebalanceState::LEN as u64;
    let bump_bytes = [bump];
    let seeds = [
        Seed::from(b"rebal".as_ref()),
        Seed::from(etf_state_key.as_ref()),
        Seed::from(bump_bytes.as_ref()),
    ];

    let existing = rebalance_state_ai.lamports();
    if existing == 0 {
        CreateAccount {
            from: payer,
            to: rebalance_state_ai,
            lamports: needed,
            space,
            owner: program_id,
        }
        .invoke_signed(&[Signer::from(&seeds)])?;
    } else {
        if existing < needed {
            Transfer {
                from: payer,
                to: rebalance_state_ai,
                lamports: needed - existing,
            }
            .invoke()?;
        }
        // Allocate must run while the account is still system-owned;
        // Assign hands ownership to this program afterwards.
        Allocate {
            account: rebalance_state_ai,
            space,
        }
        .invoke_signed(&[Signer::from(&seeds)])?;
        Assign {
            account: rebalance_state_ai,
            owner: program_id,
        }
        .invoke_signed(&[Signer::from(&seeds)])?;
    }

    let mut data = rebalance_state_ai.try_borrow_mut_data()?;
    let st = unsafe { load_mut::<RebalanceState>(&mut data) }
        .ok_or(ProgramError::InvalidAccountData)?;
    st.discriminator = RebalanceState::DISCRIMINATOR;
    st.etf_state = *etf_state_key;
    st.bump = bump;
    // CreateAccount zero-fills: window_start_slot = 0 makes the next
    // Rebalance open a fresh window; proposal_eta_slot = 0 means no
    // pending proposal.
    Ok(())
}

/// Validate an already-existing rebalance sidecar (no creation path).
/// Used by ApplyWeights, where a missing sidecar simply means no
/// proposal was ever made.
pub(crate) fn check_rebalance_state(
    program_id: &Pubkey,
    rebalance_state_ai: &AccountInfo,
    etf_state_key: &Pubkey,
) -> Result<(), ProgramError> {
    if rebalance_state_ai.owner() != program_id {
        return Err(VaultError::NoPendingProposal.into());
    }
    let (expected, _bump) =
        pubkey::find_program_address(&[b"rebal", etf_state_key], program_id);
    if rebalance_state_ai.key() != &expected {
        return Err(ProgramError::InvalidSeeds);
    }
    let data = rebalance_state_ai.try_borrow_data()?;
    let st = unsafe { load::<RebalanceState>(&data) }
        .ok_or(ProgramError::InvalidAccountData)?;
    if !st.is_initialized() || &st.etf_state != etf_state_key {
        return Err(VaultError::InvalidRebalanceState.into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn process_rebalance(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    sell_index: u8,
    buy_index: u8,
    amount_in: u64,
    min_out: u64,
    route_account_count: u8,
    route_bytes: &[u8],
) -> ProgramResult {
    if amount_in == 0 {
        return Err(VaultError::ZeroDeposit.into());
    }
    // min_out = 0 would make the buy-vault delta check vacuous — the
    // delta is the only proof the swap output reached custody rather
    // than an attacker-chosen destination account inside the route.
    if min_out == 0 {
        return Err(VaultError::SlippageExceeded.into());
    }
    if sell_index == buy_index {
        return Err(VaultError::InvalidVaultIndex.into());
    }

    if accounts.len() < FIXED_ACCOUNTS {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let authority = &accounts[0];
    let etf_state_ai = &accounts[1];
    let rebalance_state_ai = &accounts[2];
    let _system_program = &accounts[3];

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if etf_state_ai.owner() != program_id {
        return Err(VaultError::InvalidProgramOwner.into());
    }

    // ─── load etf state ────────────────────────────────────────────
    let (token_count, stored_auth, stored_bump, name_buf, token_vaults) = {
        let data = etf_state_ai.try_borrow_data()?;
        let etf =
            unsafe { load::<EtfState>(&data) }.ok_or(ProgramError::InvalidAccountData)?;
        if !etf.is_initialized() {
            return Err(VaultError::InvalidDiscriminator.into());
        }
        if etf.paused != 0 {
            return Err(VaultError::PoolPaused.into());
        }
        (
            etf.token_count as usize,
            etf.authority,
            etf.bump,
            etf.name,
            etf.token_vaults,
        )
    };

    if authority.key().as_ref() != &stored_auth {
        return Err(VaultError::OwnerMismatch.into());
    }

    let tc = token_count;
    // Defensive: `token_count` is a raw byte in EtfState and `load`
    // only checks size + discriminator, not its range. CreateEtf
    // enforces 2..=5 so this never trips today, but the local fixed-size
    // `[_; 5]` arrays below index by `tc`, so bound it explicitly rather
    // than trust the stored value (matches WithdrawSol's leg_count cap).
    if tc < 2 || tc > MAX_BASKET_TOKENS {
        return Err(VaultError::InvalidBasketSize.into());
    }
    let sell = sell_index as usize;
    let buy = buy_index as usize;
    if sell >= tc || buy >= tc {
        return Err(VaultError::InvalidVaultIndex.into());
    }

    // PDA re-derivation (SetFee idiom): a crafted writable account with
    // the right discriminator can't stand in for the real etf_state,
    // and the validated (name, bump) feed the Jupiter CPI signer below.
    let name_len = name_buf.iter().position(|&b| b == 0).unwrap_or(name_buf.len());
    let (expected_pda, expected_bump) = pubkey::find_program_address(
        &[b"etf", &stored_auth, &name_buf[..name_len]],
        program_id,
    );
    if etf_state_ai.key() != &expected_pda || expected_bump != stored_bump {
        return Err(ProgramError::InvalidSeeds);
    }

    let rac = route_account_count as usize;
    if rac + 1 > MAX_JUPITER_CPI_ACCOUNTS {
        return Err(ProgramError::InvalidArgument);
    }
    if accounts.len() < FIXED_ACCOUNTS + tc + 1 + rac {
        return Err(ProgramError::NotEnoughAccountKeys);
    }
    let jupiter_program = &accounts[FIXED_ACCOUNTS + tc];
    if jupiter_program.key().as_ref() != &JUPITER_PROGRAM_ID {
        return Err(VaultError::InvalidJupiterProgram.into());
    }

    // Vault keys must match etf.token_vaults one-for-one and be owned
    // by the SPL Token Program (same closing-the-window rationale as
    // WithdrawSol: a reassigned vault would let arbitrary bytes parse
    // as a balance).
    for i in 0..tc {
        let v = &accounts[FIXED_ACCOUNTS + i];
        if v.key() != &token_vaults[i] {
            return Err(VaultError::VaultMismatch.into());
        }
        if v.owner() != &TOKEN_PROGRAM_ID {
            return Err(VaultError::VaultMismatch.into());
        }
    }

    // ─── sidecar + turnover window ─────────────────────────────────
    ensure_rebalance_state(program_id, authority, rebalance_state_ai, etf_state_ai.key())?;

    let current_slot = Clock::get()?.slot;

    let mut vault_pre = [0u64; 5];
    for i in 0..tc {
        vault_pre[i] = read_token_account_balance(&accounts[FIXED_ACCOUNTS + i])?;
    }

    {
        let mut data = rebalance_state_ai.try_borrow_mut_data()?;
        let st = unsafe { load_mut::<RebalanceState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;

        let window_expired = st.window_start_slot == 0
            || current_slot.saturating_sub(st.window_start_slot) >= REBALANCE_WINDOW_SLOTS;
        if window_expired {
            st.window_start_slot = current_slot;
            st.window_snapshot = [0u64; 5];
            for i in 0..tc {
                st.window_snapshot[i] = vault_pre[i];
            }
            st.window_sold = [0u64; 5];
        }

        // SECURITY (M2): a zero window-open snapshot is NO LONGER lifted to
        // the live balance here. The old "zero-lift" gave a vault that was
        // empty when the window opened — and then funded mid-window by normal
        // Deposits — an immediate full turnover budget, bypassing the
        // window-open snapshot protection and amplifying the min_out=1 drain
        // (M1). A vault empty at window-open now keeps a zero budget (cap = 0,
        // so any amount_in trips TurnoverExceeded) until the window naturally
        // rolls (REBALANCE_WINDOW_SLOTS), at which point it is re-snapshotted
        // at its funded balance like every other vault.

        // Cap is computed against the window-open snapshot so deposits
        // landing mid-window can't be used to inflate the sell budget.
        let cap = (st.window_snapshot[sell] as u128)
            .checked_mul(MAX_TURNOVER_BPS as u128)
            .ok_or(VaultError::Overflow)?
            / 10_000;
        let requested = (st.window_sold[sell] as u128)
            .checked_add(amount_in as u128)
            .ok_or(VaultError::Overflow)?;
        if requested > cap {
            return Err(VaultError::TurnoverExceeded.into());
        }
    }

    // ─── Jupiter CPI (etf_state PDA signs, as in WithdrawSol) ──────
    let bump_bytes = [stored_bump];
    let pda_signer_seeds = [
        Seed::from(b"etf".as_ref()),
        Seed::from(stored_auth.as_ref()),
        Seed::from(&name_buf[..name_len]),
        Seed::from(bump_bytes.as_ref()),
    ];
    let pda_signer = Signer::from(&pda_signer_seeds);

    let route_base = FIXED_ACCOUNTS + tc + 1;
    let mut refs_storage: [core::mem::MaybeUninit<&AccountInfo>; MAX_JUPITER_CPI_ACCOUNTS] =
        unsafe { core::mem::MaybeUninit::uninit().assume_init() };
    refs_storage[0].write(&accounts[FIXED_ACCOUNTS + sell]);
    for j in 0..rac {
        refs_storage[j + 1].write(&accounts[route_base + j]);
    }
    let refs: &[&AccountInfo] = unsafe {
        core::slice::from_raw_parts(refs_storage.as_ptr() as *const &AccountInfo, rac + 1)
    };

    // etf_state PDA elevated to signer in the CPI metas — see
    // invoke_jupiter_leg docs for why the AccountInfo flag isn't enough.
    invoke_jupiter_leg(
        jupiter_program,
        refs,
        route_bytes,
        Some(&pda_signer),
        Some(etf_state_ai.key()),
    )?;

    // ─── post-CPI balance bounds ───────────────────────────────────
    // The route bytes are opaque and authority-supplied; these deltas
    // are the entire on-chain contract of what a rebalance may do.
    let mut consumed = 0u64;
    for i in 0..tc {
        let post = read_token_account_balance(&accounts[FIXED_ACCOUNTS + i])?;
        if i == sell {
            // A route depositing INTO the sell vault is harmless;
            // saturating_sub treats that as zero consumption.
            consumed = vault_pre[i].saturating_sub(post);
            if consumed > amount_in {
                return Err(VaultError::ExcessVaultDrain.into());
            }
        } else if i == buy {
            let received = post
                .checked_sub(vault_pre[i])
                .ok_or(VaultError::ExcessVaultDrain)?;
            if received == 0 {
                return Err(VaultError::JupiterCpiNoOutput.into());
            }
            if received < min_out {
                return Err(VaultError::SlippageExceeded.into());
            }
        } else if post < vault_pre[i] {
            return Err(VaultError::ExcessVaultDrain.into());
        }
    }

    // ─── account the actual consumption against the window ────────
    {
        let mut data = rebalance_state_ai.try_borrow_mut_data()?;
        let st = unsafe { load_mut::<RebalanceState>(&mut data) }
            .ok_or(ProgramError::InvalidAccountData)?;
        st.window_sold[sell] = st.window_sold[sell]
            .checked_add(consumed)
            .ok_or(VaultError::Overflow)?;
    }

    Ok(())
}
