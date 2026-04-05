pub mod pool_state;
pub mod batch_queue;
pub mod user_order_ticket;
pub mod cleared_batch_history;

pub use pool_state::PoolState3;
pub use batch_queue::BatchQueue3;
pub use user_order_ticket::UserOrderTicket3;
pub use cleared_batch_history::ClearedBatchHistory3;

pub unsafe fn load_mut<T: Copy>(data: &mut [u8]) -> Option<&mut T> {
    if data.len() < core::mem::size_of::<T>() { return None; }
    Some(&mut *(data.as_mut_ptr() as *mut T))
}

pub unsafe fn load<T: Copy>(data: &[u8]) -> Option<&T> {
    if data.len() < core::mem::size_of::<T>() { return None; }
    Some(&*(data.as_ptr() as *const T))
}
