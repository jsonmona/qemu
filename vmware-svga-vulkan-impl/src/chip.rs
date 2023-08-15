use std::{ptr::null_mut, sync::Arc, thread::JoinHandle};

use log::error;

use crate::{constants::*, ffi::chip_config::ChipConfig, fifo_processor::fifo_state::FifoState};
use std::sync::atomic::Ordering::*;

pub struct Chip {
    pub enabled: bool,
    pub pending_io_addr: u32,

    pub width: u32,
    pub height: u32,

    // Pointers in this config must be only accessed by renderer thread
    pub config: ChipConfig,

    fifo_thread: Option<JoinHandle<()>>,
    pub fifo_state: Arc<FifoState>,

    /// SVGA_ID_* constants
    negotiated_version: u32,
}

impl Chip {
    pub fn new(config: &ChipConfig) -> Self {
        assert_ne!(config.fb, null_mut(), "Framebuffer is null pointer!");
        assert_ne!(config.fifo, null_mut(), "FIFO is null pointer!");

        FifoState::init_fifo(config.fifo, config.fifo_len);
        let fifo_state = Arc::new(FifoState::new(config));

        Chip {
            enabled: false,
            pending_io_addr: 0,
            width: 0,
            height: 0,
            config: config.clone(),
            fifo_thread: None,
            fifo_state,
            negotiated_version: SVGA_VER_2,
        }
    }

    pub fn read_reg(&mut self, reg: u32) -> u32 {
        match reg {
            SVGA_REG_ID => self.negotiated_version,
            SVGA_REG_ENABLE => self.enabled as u32,
            SVGA_REG_BYTES_PER_LINE => self.width * 4,
            SVGA_REG_FB_SIZE => self.config.fb_len as u32,
            SVGA_REG_CAPABILITIES => 0,
            SVGA_REG_MEM_SIZE => self.config.fifo_len as u32,
            SVGA_REG_BUSY => self.fifo_state.busy.load(Relaxed) as u32,
            _ => {
                error!("Unknown register read [{reg}] -> 0");
                0
            }
        }
    }

    pub fn write_reg(&mut self, reg: u32, val: u32) {
        match reg {
            SVGA_REG_ID => {
                // version negotiation
                self.negotiated_version = u32::min(self.negotiated_version, val);
            }
            SVGA_REG_ENABLE => self.enabled = val != 0,
            SVGA_REG_WIDTH => {
                // should delay config until enable
                if self.fifo_state.enabled.load(Relaxed) {
                    panic!("Changed width while configured!");
                }
                self.width = val;
            }
            SVGA_REG_HEIGHT => {
                if self.fifo_state.enabled.load(Relaxed) {
                    panic!("Changed height while configured!");
                }
                self.height = val;
            }
            SVGA_REG_BITS_PER_PIXEL => {
                if val != 32 {
                    panic!("Invalid bits per depth {val}");
                }
            }
            SVGA_REG_SYNC => {
                // As documentation says...
                self.fifo_state.busy.store(true, Relaxed);
                self.start_fifo();
            }
            SVGA_REG_CONFIG_DONE => {
                let configured = val != 0;

                self.fifo_state.enabled.store(configured, SeqCst);

                if configured {
                    self.start_fifo();
                } else if let Some(x) = self.fifo_thread.take() {
                    x.join().unwrap();
                }
            }
            _ => {
                error!("Unknown register write [{reg}]={val}");
            }
        }
    }

    pub fn io_read4(&mut self, addr: u64) -> u32 {
        match addr {
            SVGA_VALUE_PORT => self.read_reg(self.pending_io_addr),
            _ => {
                println!("Unknown io port read at {addr}");
                0
            }
        }
    }

    pub fn io_write4(&mut self, addr: u64, val: u32) {
        match addr {
            SVGA_INDEX_PORT => {
                self.pending_io_addr = val;
            }
            SVGA_VALUE_PORT => {
                self.write_reg(self.pending_io_addr, val);
            }
            _ => {
                println!("Unknown io port write at {addr}={val:08x}")
            }
        }
    }

    fn start_fifo(&mut self) {
        if !self.fifo_state.enabled.load(Relaxed) {
            return;
        }

        // Already enabled. Check if thread is running.
        if let Some(x) = self.fifo_thread.as_ref() {
            self.fifo_state.resume();

            if x.is_finished() {
                if let Some(x) = self.fifo_thread.take() {
                    println!("Joining thread!");
                    x.join().unwrap();
                }
            } else {
                // No job needed
                return;
            }
        }

        let width = self.width;
        let height = self.height;
        let state = Arc::clone(&self.fifo_state);
        self.fifo_thread = Some(std::thread::spawn(move || state.run(width, height)));
    }
}
