#![allow(dead_code)]

pub const SVGA_INDEX_PORT: u64 = 0;
pub const SVGA_VALUE_PORT: u64 = 1;

pub const SVGA_VER_2: u32 = 0x90000002;

pub const SVGA_REG_ID: u32 = 0;
pub const SVGA_REG_ENABLE: u32 = 1;
pub const SVGA_REG_WIDTH: u32 = 2;
pub const SVGA_REG_HEIGHT: u32 = 3;
pub const SVGA_REG_MAX_WIDTH: u32 = 4;
pub const SVGA_REG_MAX_HEIGHT: u32 = 5;
pub const SVGA_REG_DEPTH: u32 = 6;
pub const SVGA_REG_BITS_PER_PIXEL: u32 = 7;
pub const SVGA_REG_PSEUDOCOLOR: u32 = 8;
pub const SVGA_REG_RED_MASK: u32 = 9;
pub const SVGA_REG_GREEN_MASK: u32 = 10;
pub const SVGA_REG_BLUE_MASK: u32 = 11;
pub const SVGA_REG_BYTES_PER_LINE: u32 = 12;
pub const SVGA_REG_FB_START: u32 = 13;
pub const SVGA_REG_FB_OFFSET: u32 = 14;
pub const SVGA_REG_VRAM_SIZE: u32 = 15;
pub const SVGA_REG_FB_SIZE: u32 = 16;

pub const SVGA_REG_CAPABILITIES: u32 = 17;
pub const SVGA_REG_MEM_START: u32 = 18; /* (Deprecated) */
pub const SVGA_REG_MEM_SIZE: u32 = 19;
pub const SVGA_REG_CONFIG_DONE: u32 = 20; /* Set when memory area configured */
pub const SVGA_REG_SYNC: u32 = 21; /* See "FIFO Synchronization Registers" */
pub const SVGA_REG_BUSY: u32 = 22; /* See "FIFO Synchronization Registers" */
pub const SVGA_REG_GUEST_ID: u32 = 23; /* Set guest OS identifier */
pub const SVGA_REG_CURSOR_ID: u32 = 24; /* (Deprecated) */
pub const SVGA_REG_CURSOR_X: u32 = 25; /* (Deprecated) */
pub const SVGA_REG_CURSOR_Y: u32 = 26; /* (Deprecated) */
pub const SVGA_REG_CURSOR_ON: u32 = 27; /* (Deprecated) */
pub const SVGA_REG_HOST_BITS_PER_PIXEL: u32 = 28; /* (Deprecated) */
pub const SVGA_REG_SCRATCH_SIZE: u32 = 29; /* Number of scratch registers */
pub const SVGA_REG_MEM_REGS: u32 = 30; /* Number of FIFO registers */
pub const SVGA_REG_NUM_DISPLAYS: u32 = 31; /* (Deprecated) */
pub const SVGA_REG_PITCHLOCK: u32 = 32; /* Fixed pitch for all modes */
pub const SVGA_REG_IRQMASK: u32 = 33; /* Interrupt mask */
