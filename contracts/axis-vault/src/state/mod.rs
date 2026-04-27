pub mod etf;

pub use etf::{EtfState, MAX_BASKET_TOKENS, MAX_ETF_NAME_LEN, MAX_ETF_TICKER_LEN};

/// Safely transmute a `&mut [u8]` to `&mut T`. Returns `None` on size or
/// alignment mismatch.
///
/// Solana account data buffers are 8-byte aligned by the runtime — this
/// is enough for any `repr(C)` struct whose largest field is a `u64`,
/// but not stronger types. Adding the runtime alignment check costs two
/// instructions per call and protects against future struct-layout
/// changes that might bump alignment beyond what the runtime promises.
/// A misaligned read is undefined behaviour in Rust; release builds may
/// silently misoptimise around it. The guard is cheap insurance.
///
/// # Safety
/// Caller must guarantee the type `T` matches the on-chain layout for
/// this byte buffer (correct discriminator, correct version).
pub unsafe fn load_mut<T: Copy>(data: &mut [u8]) -> Option<&mut T> {
    if data.len() < core::mem::size_of::<T>() {
        return None;
    }
    if (data.as_mut_ptr() as usize) % core::mem::align_of::<T>() != 0 {
        return None;
    }
    Some(&mut *(data.as_mut_ptr() as *mut T))
}

/// Safely transmute a `&[u8]` to `&T`. See `load_mut` for invariants.
pub unsafe fn load<T: Copy>(data: &[u8]) -> Option<&T> {
    if data.len() < core::mem::size_of::<T>() {
        return None;
    }
    if (data.as_ptr() as usize) % core::mem::align_of::<T>() != 0 {
        return None;
    }
    Some(&*(data.as_ptr() as *const T))
}
