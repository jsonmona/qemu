use crate::shared_mem::SharedMem;
use std::{ptr::slice_from_raw_parts, sync::atomic::Ordering::*};

// What a bad place to put constants :P
// (and now duplicated over places!)
const SVGA_FIFO_MIN: u32 = 0;
const SVGA_FIFO_MAX: u32 = 1;
const SVGA_FIFO_NEXT_CMD: u32 = 2;
const SVGA_FIFO_STOP: u32 = 3;

/**
Memory safety of accessing FIFO:

Guest driver will (hopefully) write NEXT_CMD with release semantic,
and read STOP with acquire semantic. Plus, they (hopefully) won't
touch host-owned portion of memory.

That allows us to treat those two variables as mutex into FIFO commands.
 */
pub struct FifoReader<'cb> {
    mem: SharedMem<u32>,

    /** terminate on getting true */
    suspend: &'cb mut dyn FnMut() -> bool,

    /** Borrowed commands but did not advance STOP by this amount */
    borrowed_cmds: u32,

    buffer: Box<[u32; 1024]>,

    // As far as I understand, these won't be touched after configured
    min: u32,
    max: u32,
    fifo_bytes: u32,
}

impl FifoReader<'_> {
    pub fn new(mem: SharedMem<u32>, suspend: &mut dyn FnMut() -> bool) -> FifoReader<'_> {
        let min = mem.at(SVGA_FIFO_MIN as usize).load(Relaxed);
        let max = mem.at(SVGA_FIFO_MAX as usize).load(Relaxed);
        assert!(min < max, "inverted range");

        let fifo_bytes = max - min;
        assert_eq!(fifo_bytes % 4, 0, "fifo size not divisable by 4");

        FifoReader {
            mem,
            suspend,
            borrowed_cmds: 0,
            buffer: bytemuck::zeroed_box(),
            min,
            max,
            fifo_bytes,
        }
    }

    pub fn available(&self) -> usize {
        let stop = self.mem.at(SVGA_FIFO_STOP as usize).load(Relaxed);
        let next_cmd = self.mem.at(SVGA_FIFO_NEXT_CMD as usize).load(Relaxed);

        assert!(
            self.min <= stop && self.min <= next_cmd && stop < self.max && next_cmd < self.max,
            "invalid head or tail"
        );

        match stop.cmp(&next_cmd) {
            std::cmp::Ordering::Less => (next_cmd - stop - self.borrowed_cmds) as usize / 4,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => {
                (((self.max - stop) + (next_cmd - self.min)) / 4 - self.borrowed_cmds) as usize
            }
        }
    }

    /** returns None if needs to terminate */
    pub fn borrow(&mut self, cmds: usize) -> Option<&[u32]> {
        while self.available() < cmds {
            (self.suspend)();
        }

        Some(
            self.try_borrow(cmds)
                .expect("checked for remaining commands"),
        )
    }

    /** returns None if not enough data is available (does not block) */
    pub fn try_borrow(&mut self, cmds: usize) -> Option<&[u32]> {
        assert!(cmds < self.buffer.len(), "borrow request too large");

        let cmds = cmds as u32;

        let stop = self.cleanup_borrow();
        let next_cmd = self.mem.at(SVGA_FIFO_NEXT_CMD as usize).load(Acquire);

        if stop == next_cmd {
            return None;
        }

        let effective_end = if stop <= next_cmd { next_cmd } else { self.max };

        if stop + cmds * 4 <= effective_end {
            // Fast path. Return slice directly
            self.borrowed_cmds = cmds;
            return Some(self.make_fifo_slice(&self.mem, stop, cmds));
        }

        let wrap_remain = stop + cmds * 4 - effective_end;
        if next_cmd < stop && wrap_remain + self.min <= next_cmd {
            // Slow path. Copy into buffer with two memcpy
            let mid_cmd = cmds - wrap_remain / 4;
            debug_assert_eq!(wrap_remain / 4 + mid_cmd, cmds);

            let first_src = self.make_fifo_slice(&self.mem, stop, mid_cmd);
            let second_src = self.make_fifo_slice(&self.mem, self.min, wrap_remain / 4);

            let (first, remain) = self.buffer.split_at_mut(mid_cmd as usize);
            let (second, _) = remain.split_at_mut(wrap_remain as usize / 4);

            first.copy_from_slice(first_src);
            second.copy_from_slice(second_src);

            self.advance(cmds);
            return Some(&self.buffer[..cmds as usize]);
        }

        // Not enough data
        None
    }

    /** STOP and NEXT_CMD must be acquired before calling this function */
    fn make_fifo_slice<'a>(&self, mem: &'a SharedMem<u32>, from_byte: u32, cmds: u32) -> &'a [u32] {
        assert_eq!(
            &self.mem as *const _, mem as *const _,
            "call this function with self.mem"
        );
        assert!(from_byte < self.max, "buffer overflow");

        unsafe {
            let p = mem.as_byte_ptr().add(from_byte as usize);
            &*slice_from_raw_parts(p as *mut u32, cmds as usize)
        }
    }

    /** returns STOP value, loaded with acquire ordering */
    fn cleanup_borrow(&mut self) -> u32 {
        if self.borrowed_cmds != 0 {
            let stop = self.advance(self.borrowed_cmds);
            self.borrowed_cmds = 0;
            stop
        } else {
            self.mem.at(SVGA_FIFO_STOP as usize).load(Acquire)
        }
    }

    /** returns STOP value, loaded with acquire ordering */
    fn advance(&mut self, amount: u32) -> u32 {
        let stop = self.mem.at(SVGA_FIFO_STOP as usize);

        let mut old_pos = stop.load(Acquire);
        loop {
            let new_pos = (old_pos + amount * 4 - self.min) % self.fifo_bytes + self.min;
            match stop.compare_exchange_weak(old_pos, new_pos, Release, Acquire) {
                Ok(_) => return new_pos,
                Err(x) => old_pos = x,
            }
        }
    }

    /** Returns None if need to terminate */
    pub fn next(&mut self) -> Option<u32> {
        //FIXME: Quite inefficient on weakly ordered CPU

        let mut old_pos = self.cleanup_borrow();
        let stop = self.mem.at(SVGA_FIFO_STOP as usize);

        loop {
            let next_cmd = self.mem.at(SVGA_FIFO_NEXT_CMD as usize).load(Acquire);

            if old_pos == next_cmd {
                if (self.suspend)() {
                    return None;
                }
                continue;
            }

            let cmd = self.mem.at(old_pos as usize / 4).load(Relaxed);
            let new_pos = (old_pos + 4 - self.min) % self.fifo_bytes + self.min;
            match stop.compare_exchange_weak(old_pos, new_pos, Release, Acquire) {
                Ok(_) => return Some(cmd),
                Err(x) => old_pos = x,
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn suspend() -> bool {
        false
    }

    fn setup<'cb>(
        arr: &mut [u32; 10],
        cmds: u32,
        suspend_fn: &'cb mut dyn FnMut() -> bool,
    ) -> FifoReader<'cb> {
        arr[0] = 4 * 4; // MIN
        arr[1] = 10 * 4; // MAX
        arr[2] = (4 + cmds) * 4; // NEXT_CMD
        arr[3] = arr[0]; // STOP

        if arr[1] <= arr[2] {
            arr[2] -= 6 * 4;
        }

        // values in [1 ~ 6] range
        for i in 4..10 {
            arr[i] = (i - 3) as u32;
        }

        let mem = SharedMem::<u32>::new(arr.as_mut_ptr() as *mut u8, arr.len() * 4);

        FifoReader::new(mem, suspend_fn)
    }

    #[test]
    fn basic_iter() {
        let suspend = &mut suspend;
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 5, suspend);

        assert_eq!(reader.available(), 5);
        assert_eq!(reader.next(), Some(1));
        assert_eq!(reader.next(), Some(2));
        assert_eq!(reader.next(), Some(3));
        assert_eq!(reader.next(), Some(4));
        assert_eq!(reader.next(), Some(5));
        assert_eq!(reader.available(), 0);
    }

    #[test]
    fn wrapping_iter() {
        let suspend = &mut suspend;
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 7, suspend);

        arr[3] += 8; // Advance 2 commands

        assert_eq!(reader.available(), 5);
        assert_eq!(reader.next(), Some(3));

        assert_eq!(reader.available(), 4);
        assert_eq!(reader.next(), Some(4));

        assert_eq!(reader.available(), 3);
        assert_eq!(reader.next(), Some(5));

        assert_eq!(reader.available(), 2);
        assert_eq!(reader.next(), Some(6));

        assert_eq!(reader.available(), 1);
        assert_eq!(reader.next(), Some(1));

        assert_eq!(reader.available(), 0);
    }

    #[test]
    fn basic_slice() {
        let suspend = &mut suspend;
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 5, suspend);

        assert_eq!(reader.try_borrow(6), None);
        assert_eq!(reader.try_borrow(3), Some([1, 2, 3].as_slice()));
        assert_eq!(reader.try_borrow(2), Some([4, 5].as_slice()));
        assert_eq!(reader.try_borrow(2), None);
        assert_eq!(reader.try_borrow(1), None);
    }

    #[test]
    fn wrapping_slice() {
        let suspend = &mut suspend;
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 7, suspend);

        arr[3] += 8; // Advance 2 commands

        assert_eq!(reader.try_borrow(6), None);
        assert_eq!(reader.try_borrow(2), Some([3, 4].as_slice()));
        assert_eq!(reader.try_borrow(3), Some([5, 6, 1].as_slice()));
        assert_eq!(reader.try_borrow(1), None);
        assert_eq!(reader.try_borrow(2), None);

        // 2nd test
        reader = setup(&mut arr, 7, suspend);

        arr[3] += 8; // Advance 2 commands

        assert_eq!(reader.try_borrow(6), None);
        assert_eq!(reader.try_borrow(4), Some([3, 4, 5, 6].as_slice()));
        assert_eq!(reader.try_borrow(1), Some([1].as_slice()));
        assert_eq!(reader.try_borrow(1), None);
        assert_eq!(reader.try_borrow(2), None);
    }
}
