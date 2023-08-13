use std::ptr::null_mut;

#[repr(C)]
#[derive(Clone)]
pub struct ChipConfig {
    pub fifo: *mut u8,
    pub fb: *mut u8,
    pub fifo_len: usize,
    pub fb_len: usize,
    pub vram_len: usize,
}

// Config itself should be thread free
unsafe impl Send for ChipConfig {}
unsafe impl Sync for ChipConfig {}

impl Default for ChipConfig {
    fn default() -> Self {
        ChipConfig {
            fifo: null_mut(),
            fb: null_mut(),
            fifo_len: 0,
            fb_len: 0,
            vram_len: 128 * 1024 * 1024,
        }
    }
}
