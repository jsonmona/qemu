use std::fmt::Debug;

use bytemuck::{Pod, Zeroable};

use crate::ref_or_box::RefOrBox;

use super::{fifo_reader::FifoReader, fifo_state::FifoState};

pub trait FifoCmdBuildable: Clone {
    /** Opcode of this command */
    const OPCODE: u32;

    /** Number of argument this command requires. None if dynamic. */
    const ARGS: Option<u32>;

    /** Name of this command */
    const NAME: &'static str;

    /** Make an instance of this command from fifo stream */
    fn from_fifo<'a>(fifo: &'a mut FifoReader) -> Option<RefOrBox<'a, dyn FifoCmd>>;
}

pub trait FifoCmdInfo {
    /** Opcode of this command */
    fn opcode(&self) -> u32;

    /** Name of this command */
    fn name(&self) -> &'static str;
}

pub trait FifoCmd: Debug + FifoCmdInfo {
    /** Process this command */
    fn process(&self, state: &FifoState);
}

impl<T: FifoCmdBuildable> FifoCmdInfo for T {
    fn opcode(&self) -> u32 {
        Self::OPCODE
    }
    fn name(&self) -> &'static str {
        Self::NAME
    }
}

#[derive(Debug, Clone, Copy, Zeroable, Pod)]
#[repr(C)]
pub struct FifoCmdUpdate {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl FifoCmdBuildable for FifoCmdUpdate {
    const OPCODE: u32 = 1;
    const ARGS: Option<u32> = Some(4);
    const NAME: &'static str = "SVGA_CMD_UPDATE";

    fn from_fifo<'a>(fifo: &'a mut FifoReader) -> Option<RefOrBox<'a, dyn FifoCmd>> {
        let cmd = bytemuck::cast_slice(fifo.borrow(4)?);
        Some(RefOrBox::from_ref(bytemuck::from_bytes::<Self>(cmd)))
    }
}

impl FifoCmd for FifoCmdUpdate {
    fn process(&self, _state: &FifoState) {
        // Do nothing
        //TODO: Mark the region as dirty
    }
}

macro_rules! unimplemented_fifo_cmd {
    ($type_name:ident, $name:ident, $opcode:literal, $args:literal) => {
        #[derive(Clone)]
        pub struct $type_name;
        impl Debug for $type_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let opcode: u32 = $opcode;
                let args: u32 = $args;
                f.debug_struct(stringify!($type_name))
                    .field("name", &Self::NAME)
                    .field("opcode", &opcode)
                    .field("args", &args)
                    .finish()
            }
        }
        impl FifoCmdBuildable for $type_name {
            const OPCODE: u32 = $opcode;
            const ARGS: Option<u32> = Some($args);
            const NAME: &'static str = stringify!($name);
            fn from_fifo<'a>(fifo: &'a mut FifoReader) -> Option<RefOrBox<'a, dyn FifoCmd>> {
                for _ in 0..$args {
                    fifo.next()?;
                }
                Some(Box::new($type_name).into())
            }
        }
        impl FifoCmd for $type_name {
            fn process(&self, _state: &FifoState) {
                println!("STUB: {}", Self::NAME);
            }
        }
    };
}

unimplemented_fifo_cmd! { FifoCmdFence, SVGA_CMD_FENCE, 30, 1 }

pub fn fetch_fifo_cmd<'a>(fifo: &'a mut FifoReader<'_>) -> Option<RefOrBox<'a, dyn FifoCmd>> {
    let cmd: u32 = fifo.next()?;

    match cmd {
        FifoCmdUpdate::OPCODE => FifoCmdUpdate::from_fifo(fifo),
        FifoCmdFence::OPCODE => FifoCmdFence::from_fifo(fifo),
        _ => {
            panic!("unknown FIFO command: {cmd}");
        }
    }
}
