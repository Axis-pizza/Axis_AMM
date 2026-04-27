pub mod batch_queue;
pub mod cleared_batch_history;
pub mod pool_state;
pub mod user_order_ticket;

pub use batch_queue::BatchQueue;
pub use cleared_batch_history::ClearedBatchHistory;
pub use pool_state::PoolState;
pub use user_order_ticket::UserOrderTicket;

/// Safely cast a mutable byte slice to a typed struct reference.
/// Requires the slice to be at least `size_of::<T>()` bytes AND
/// aligned to `align_of::<T>()`.
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
/// The caller must ensure the byte slice comes from an account data buffer
/// owned by the program and has the correct discriminator + version.
pub unsafe fn load_mut<T: Copy>(data: &mut [u8]) -> Option<&mut T> {
    if data.len() < core::mem::size_of::<T>() {
        return None;
    }
    if (data.as_mut_ptr() as usize) % core::mem::align_of::<T>() != 0 {
        return None;
    }
    let ptr = data.as_mut_ptr() as *mut T;
    Some(&mut *ptr)
}

/// Safely cast a byte slice to a typed struct reference (immutable).
/// See `load_mut` for invariants.
pub unsafe fn load<T: Copy>(data: &[u8]) -> Option<&T> {
    if data.len() < core::mem::size_of::<T>() {
        return None;
    }
    if (data.as_ptr() as usize) % core::mem::align_of::<T>() != 0 {
        return None;
    }
    let ptr = data.as_ptr() as *const T;
    Some(&*ptr)
}
