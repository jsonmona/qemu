use std::ptr::{null_mut, slice_from_raw_parts_mut};
use std::sync::atomic::Ordering::*;
use std::sync::Arc;
use std::{sync::atomic::AtomicBool, time::Duration};

use log::{trace, warn};
use parking_lot::{Condvar, Mutex};

use crate::ffi::chip_config::ChipConfig;
use crate::graphic::GraphicState;
use crate::mailbox::Mailbox;
use crate::shared_mem::SharedMem;

use super::cmd::fetch_fifo_cmd;
use super::fifo_reader::FifoReader;

//FIXME: Not sure why this exists at all
const MAGIC_OFFSET: usize = 2;

/**
 * Shared state between Chip and Fifo thread
 */
pub struct FifoState {
    pub fifo: SharedMem<u32>,
    pub fb: SharedMem<u32>,
    pub enabled: AtomicBool,
    pub busy: AtomicBool,

    resume: Condvar,
    resume_mutex: Mutex<()>,
    output: Arc<Mailbox>,
}

impl FifoState {
    pub fn new(config: &ChipConfig) -> Self {
        let adjusted_ptr = unsafe { config.fifo.add(MAGIC_OFFSET * 4) };

        FifoState {
            fifo: SharedMem::new(adjusted_ptr, config.fifo_len),
            fb: SharedMem::new(config.fb, config.fb_len),
            enabled: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            resume: Default::default(),
            resume_mutex: Mutex::new(()),
            output: Mailbox::new(),
        }
    }

    pub fn init_fifo(fifo: *mut u8, len: usize) {
        assert_ne!(fifo, null_mut(), "FIFO null pointer");
        assert!(len > 32, "FIFO too small");
        assert_eq!(len % 4, 0, "FIFO size must be divisable by 4");

        // Accessing memory while unconfigured is safe
        let fifo: &mut [u32] = unsafe {
            let arr = &mut *slice_from_raw_parts_mut(fifo, len);
            bytemuck::cast_slice_mut(arr)
        };

        fifo.fill(0);
    }

    pub fn read_output(&self, ptr: *mut u8, len: usize) -> bool {
        if ptr.is_null() {
            return false;
        }

        let img = self.output.borrow_read();
        match img.as_ref() {
            Some(data) => {
                if data.len() * 4 != len {
                    warn!(
                        "Output buffer size mismatch: {} vs {} expected",
                        len,
                        data.len() * 4
                    );
                    return false;
                }

                unsafe {
                    ptr.copy_from_nonoverlapping(data.as_ptr() as *const u8, len);
                }

                true
            }
            None => false,
        }
    }

    pub fn run(&self, width: u32, height: u32) {
        let mut suspend = || self.suspend();
        let mut fifo = FifoReader::new(self.fifo.clone(), &mut suspend);

        let mut graphic = pollster::block_on(GraphicState::new(width, height));

        while self.enabled.load(Acquire) {
            self.render_output(width, height, &mut graphic);

            let cmd = match fetch_fifo_cmd(&mut fifo) {
                Some(x) => x,
                None => {
                    continue;
                }
            };

            //println!("{:?}", cmd);
            cmd.process(self, &mut graphic);
        }
    }

    pub fn resume(&self) {
        self.resume.notify_all();
    }

    fn render_output(&self, width: u32, height: u32, grpahic: &mut GraphicState) {
        let output_pixels = (width as usize) * (height as usize);

        if output_pixels == 0 {
            return;
        }

        let mut img = self.output.borrow_write();
        if output_pixels != img.as_ref().map(|x| x.len()).unwrap_or(0) {
            *img = Some(vec![0; output_pixels]);
        }

        let dst = img.as_mut().expect("checked").as_mut_slice();
        let dst_bytes = bytemuck::cast_slice_mut(dst);

        pollster::block_on(grpahic.render(dst_bytes));
    }

    // Returns true if needs to terminate
    fn suspend(&self) -> bool {
        //TODO: Save VRAM before sleeping
        self.busy.store(false, Relaxed);
        trace!("FIFO processor going to sleep");

        let mut guard = self.resume_mutex.lock();
        self.resume.wait_for(&mut guard, Duration::from_secs(5));
        !self.enabled.load(Relaxed)
    }
}
