pub mod etf;

pub use etf::{EtfState, MAX_BASKET_TOKENS, MAX_ETF_NAME_LEN, MAX_ETF_TICKER_LEN};

pub unsafe fn load_mut<T: Copy>(data: &mut [u8]) -> Option<&mut T> {
    if data.len() < core::mem::size_of::<T>() { return None; }
    Some(&mut *(data.as_mut_ptr() as *mut T))
}

pub unsafe fn load<T: Copy>(data: &[u8]) -> Option<&T> {
    if data.len() < core::mem::size_of::<T>() { return None; }
    Some(&*(data.as_ptr() as *const T))
}
