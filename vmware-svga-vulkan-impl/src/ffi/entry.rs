use std::mem::align_of;
use std::ptr::drop_in_place;
use std::sync::atomic::Ordering::SeqCst;
use std::{mem::size_of, ops::DerefMut, ptr::null_mut};

use parking_lot::Mutex;

use crate::chip::Chip;
use crate::ffi::chip_config::ChipConfig;

#[allow(non_camel_case_types)]
pub struct vmsvga_vk_impl(Mutex<Chip>);

fn lock_ptr(p: Option<&vmsvga_vk_impl>) -> impl DerefMut<Target = Chip> + '_ {
    p.expect("Got null pointer through FFI").0.lock()
}

// ===== Instance create & destroy =====

/**
Overwrites config with sensible default values.

# Safety
`config` should be either null or a valid pointer to a ChipConfig.
Size mismatch won't cause memory unsoundness.
 */
#[no_mangle]
pub unsafe extern "C" fn vmsvga_vk_config_default(config_size: usize, config: *mut ChipConfig) {
    // Ignore error
    let _ = pretty_env_logger::try_init();

    //TODO: Should return an error instead of aborting
    assert_eq!(config_size, size_of::<ChipConfig>(), "Invalid size");
    assert!(!config.is_null(), "Should not be a null pointer");
    assert_eq!(
        config.align_offset(align_of::<ChipConfig>()),
        0,
        "Invalid alignment"
    );

    // Checked everything. It's safe to write.
    unsafe {
        config.write(Default::default());
    }
}

#[no_mangle]
pub extern "C" fn vmsvga_vk_new(config: &ChipConfig) -> Box<vmsvga_vk_impl> {
    // Ignore error
    let _ = pretty_env_logger::try_init();

    Box::new(vmsvga_vk_impl(Mutex::new(Chip::new(config))))
}

/**
# Safety
`ptr` must a valid pointer.
The pointer pointed by `ptr` (The *vmsvga_vk_impl) may be null.
 */
#[no_mangle]
pub unsafe extern "C" fn vmsvga_vk_freep(ptr: *mut *mut vmsvga_vk_impl) {
    let chip_ptr = *ptr;
    *ptr = null_mut();
    std::sync::atomic::fence(SeqCst); // Just to be safe
    drop_in_place(chip_ptr);
}

// ===== Various query (read-only) operations =====

#[no_mangle]
pub extern "C" fn vmsvga_vk_is_vga_mode(p: Option<&vmsvga_vk_impl>) -> bool {
    let chip = lock_ptr(p);
    !chip.enabled
}

// ===== IO operations =====

#[no_mangle]
pub extern "C" fn vmsvga_vk_read_io4(p: Option<&vmsvga_vk_impl>, addr: u64) -> u32 {
    let mut chip = lock_ptr(p);
    chip.io_read4(addr)
}

#[no_mangle]
pub extern "C" fn vmsvga_vk_write_io4(p: Option<&vmsvga_vk_impl>, addr: u64, val: u32) {
    let mut chip = lock_ptr(p);
    chip.io_write4(addr, val);
}

// ===== Output operations =====

#[no_mangle]
pub extern "C" fn vmsvga_vk_invalidate(_p: Option<&vmsvga_vk_impl>) {
    println!("STUB: vmsvga_vk_invalidate")
}

#[no_mangle]
pub extern "C" fn vmsvga_vk_output_info(
    p: Option<&vmsvga_vk_impl>,
    w: &mut u32,
    h: &mut u32,
    stride: &mut u32,
) {
    let chip = lock_ptr(p);
    *w = chip.width;
    *h = chip.height;
    *stride = chip.width * 4;
}

/**
 * @return false if no output has been produced
 */
#[no_mangle]
pub extern "C" fn vmsvga_vk_output_read(
    p: Option<&vmsvga_vk_impl>,
    ptr: *mut u8,
    len: usize,
) -> bool {
    let chip = lock_ptr(p);
    chip.fifo_state.read_output(ptr, len)
}
