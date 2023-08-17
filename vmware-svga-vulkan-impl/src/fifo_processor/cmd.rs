use std::{fmt::Debug, ops::Deref};

use bytemuck::{Pod, Zeroable};

use crate::{graphic::GraphicState, ref_or_box::RefOrBox};

use super::{fifo_reader::FifoView, fifo_state::FifoState};

pub trait FifoCmdBuildable: Clone {
    /** Opcode of this command */
    const OPCODE: u32;

    /** Number of argument this command requires. None if dynamic. */
    const ARGS: Option<u32>;

    /** Name of this command */
    const NAME: &'static str;

    /** Make an instance of this command from fifo stream */
    fn from_fifo<'a>(view: &'a mut FifoView) -> Option<RefOrBox<'a, dyn FifoCmd>>;
}

pub trait FifoCmdInfo {
    /** Opcode of this command */
    fn opcode(&self) -> u32;

    /** Name of this command */
    fn name(&self) -> &'static str;
}

pub trait FifoCmd: Debug + FifoCmdInfo {
    /** Process this command */
    fn process(&self, state: &FifoState, graphic: &mut GraphicState);
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

    fn from_fifo<'a>(view: &'a mut FifoView) -> Option<RefOrBox<'a, dyn FifoCmd>> {
        Some(match view.borrow(4)? {
            RefOrBox::Refed(x) => RefOrBox::from_ref(bytemuck::from_bytes::<Self>(bytemuck::cast_slice(x))),
            RefOrBox::Boxed(x) => {
                //FIXME: Is this even remotely safe?
                let q: Box<[FifoCmdUpdate]> = bytemuck::cast_slice_box(x);
                let c: Box<FifoCmdUpdate> = unsafe { Box::from_raw(Box::into_raw(q) as *mut FifoCmdUpdate) };
                RefOrBox::from_box(c)
            }
        })
    }
}

impl FifoCmd for FifoCmdUpdate {
    fn process(&self, state: &FifoState, graphic: &mut GraphicState) {
        // TODO: Delay and do partial update
        let pixels = graphic.width() * graphic.height();
        let data = state.fb.slice_to(0, pixels as usize);
        graphic.cmd_update_framebuffer_whole(data);
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
            fn from_fifo<'a>(view: &'a mut FifoView) -> Option<RefOrBox<'a, dyn FifoCmd>> {
                for _ in 0..$args {
                    view.next()?;
                }
                Some(Box::new($type_name).into())
            }
        }
        impl FifoCmd for $type_name {
            fn process(&self, _state: &FifoState, _graphic: &mut GraphicState) {
                println!("STUB: {}", Self::NAME);
            }
        }
    };
}

unimplemented_fifo_cmd! { FifoCmdFence, SVGA_CMD_FENCE, 30, 1 }

pub fn fetch_fifo_cmd<'a>(view: &'a mut FifoView) -> Option<RefOrBox<'a, dyn FifoCmd>> {
    let opcode = view.next()?;

    match opcode {
        FifoCmdUpdate::OPCODE => FifoCmdUpdate::from_fifo(view),
        FifoCmdFence::OPCODE => FifoCmdFence::from_fifo(view),
        _ => {
            panic!("unknown FIFO command: {opcode}");
        }
    }
}
