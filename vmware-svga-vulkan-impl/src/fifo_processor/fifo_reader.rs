use bytemuck::Zeroable;

use crate::{shared_mem::SharedMem, ref_or_box::RefOrBox};
use std::sync::atomic::Ordering::*;

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
pub struct FifoReader {
    mem: SharedMem<u32>,

    // As far as I understand, these won't be touched after configured
    min_byte: u32,
    max_byte: u32,
    fifo_bytes: u32,
    min_idx: u32,
    max_idx: u32,
    fifo_len: u32,
}

impl FifoReader {
    pub fn new(mem: SharedMem<u32>) -> FifoReader {
        let min = mem.at(SVGA_FIFO_MIN as usize).load(Acquire);
        let max = mem.at(SVGA_FIFO_MAX as usize).load(Acquire);

        assert!(min < max, "inverted range");
        assert_eq!(min % 4, 0, "FIFO bounds not divisable by 4");
        assert_eq!(max % 4, 0, "FIFO bounds not divisable by 4");

        FifoReader {
            mem,
            min_byte: min,
            max_byte: max,
            fifo_bytes: max - min,
            min_idx: min / 4,
            max_idx: max / 4,
            fifo_len: (max - min) / 4,
        }
    }

    /** Returns (available, stop) */
    fn available(&self) -> (u32, u32) {
        let stop = self.mem.at(SVGA_FIFO_STOP as usize).load(Acquire);
        let next_cmd = self.mem.at(SVGA_FIFO_NEXT_CMD as usize).load(Acquire);

        assert!(
            self.min_byte <= stop && self.min_byte <= next_cmd && stop < self.max_byte && next_cmd < self.max_byte,
            "invalid head or tail"
        );

        let available = match u32::cmp(&stop, &next_cmd) {
            std::cmp::Ordering::Less => (next_cmd - stop) / 4,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => {
                ((self.max_byte - stop) + (next_cmd - self.min_byte)) / 4
            }
        };

        (available, stop)
    }

    pub fn view(&mut self) -> FifoView<'_> {
        let (available, stop) = self.available();

        FifoView {
            parent: self,
            peeked_amount: 0,
            available,
            cmd_pos: stop / 4,
        }
    }

    /** returns STOP value, loaded with acquire ordering */
    fn advance(&self, amount: u32) -> u32 {
        let stop = self.mem.at(SVGA_FIFO_STOP as usize);

        let mut old_pos = stop.load(Acquire);
        loop {
            let new_pos = (old_pos + amount * 4 - self.min_byte) % self.fifo_bytes + self.min_byte;
            match stop.compare_exchange_weak(old_pos, new_pos, Release, Acquire) {
                Ok(_) => return new_pos,
                Err(x) => old_pos = x,
            }
        }
    }
}

pub struct FifoView<'fifo> {
    parent: &'fifo FifoReader,
    peeked_amount: u32,
    available: u32,
    cmd_pos: u32,
}

impl<'fifo> FifoView<'fifo> {
    /** Commit and consume viewed data */
    pub fn commit(self) {
        self.parent.advance(self.peeked_amount);
    }

    pub fn available(&self) -> u32 {
        self.available - self.peeked_amount
    }

    pub fn next(&mut self) -> Option<u32> {
        if self.peeked_amount + 1 <= self.available {
            let x = self.parent.mem.read_volatile(self.cmd_pos as usize);
            self.advance(1);
            Some(x)
        } else {
            None
        }
    }

    pub fn borrow(&mut self, amount: u32) -> Option<RefOrBox<'fifo, [u32]>> {
        if self.available < self.peeked_amount + amount {
            return None;
        }

        let dist_to_max = self.parent.max_idx - self.cmd_pos;
        if amount <= dist_to_max {
            // Fast path. Return slice directly
            let data = self.parent.mem.slice_to(self.cmd_pos as usize, (self.cmd_pos + amount) as usize);
            
            self.advance(amount);
            Some(RefOrBox::Refed(data))
        } else {
            // Slow path. Copy to box with two memcpy
            let mut data = alloc_box_slice(amount as usize);

            let first_half = self.parent.mem.slice_to(self.cmd_pos as usize, (self.cmd_pos + dist_to_max) as usize);
            (&mut data[..(dist_to_max as usize)]).copy_from_slice(first_half);

            let second_half_len = amount - dist_to_max;
            let second_half = self.parent.mem.slice_to(self.parent.min_idx as usize, (self.parent.min_idx + second_half_len) as usize);
            (&mut data[(dist_to_max as usize)..]).copy_from_slice(second_half);

            self.advance(amount);
            Some(RefOrBox::Boxed(data))
        }
    }

    fn advance(&mut self, amount: u32) {
        self.peeked_amount += amount;
        self.cmd_pos += amount;
        while self.parent.max_idx <= self.cmd_pos {
            self.cmd_pos -= self.parent.fifo_len;
        }
    }
}

fn alloc_box_slice<T: Copy + Zeroable>(len: usize) -> Box<[T]> {
    if len == 0 {
        return Default::default();
    }
    let layout = std::alloc::Layout::array::<T>(len).unwrap();
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) as *mut T };
    let slice_ptr = std::ptr::slice_from_raw_parts_mut(ptr, len);
    unsafe { Box::from_raw(slice_ptr) }
}

#[cfg(test)]
mod test {
    use super::*;

    fn setup(
        arr: &mut [u32; 10],
        cmds: u32
    ) -> FifoReader {
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

        FifoReader::new(mem)
    }

    #[test]
    fn basic_iter() {
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 5);

        assert_eq!(reader.available().0, 5);
        let mut view = reader.view();
        assert_eq!(view.next(), Some(1));
        assert_eq!(view.next(), Some(2));
        assert_eq!(view.next(), Some(3));
        view.commit();

        assert_eq!(reader.available().0, 2);
        let mut view = reader.view();
        assert_eq!(view.next(), Some(4));
        assert_eq!(view.next(), Some(5));
        assert_eq!(view.next(), None);
        // Does not commit

        assert_eq!(reader.available().0, 2);
        let mut view = reader.view();
        assert_eq!(view.next(), Some(4));
        assert_eq!(view.next(), Some(5));
        assert_eq!(view.next(), None);
        view.commit();

        assert_eq!(reader.available().0, 0);
    }

    #[test]
    fn wrapping_iter() {
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 7);

        arr[3] += 8; // Advance 2 commands

        assert_eq!(reader.available().0, 5);
        let mut view = reader.view();
        assert_eq!(view.next(), Some(3));
        assert_eq!(view.next(), Some(4));
        assert_eq!(view.next(), Some(5));
        view.commit();

        assert_eq!(reader.available().0, 2);
        let mut view = reader.view();
        assert_eq!(view.next(), Some(6));
        assert_eq!(view.next(), Some(1));
        assert_eq!(view.next(), None);
        // Does not commit

        assert_eq!(reader.available().0, 2);
        let mut view = reader.view();
        assert_eq!(view.next(), Some(6));
        assert_eq!(view.next(), Some(1));
        assert_eq!(view.next(), None);
        view.commit();

        assert_eq!(reader.available().0, 0);
    }

    #[test]
    fn basic_slice() {
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 5);

        let mut view = reader.view();
        assert_eq!(view.borrow(6), None);
        assert_eq!(view.borrow(3), Some(RefOrBox::Refed([1, 2, 3].as_slice())));
        view.commit();

        let mut view = reader.view();
        assert_eq!(view.borrow(2), Some(RefOrBox::Refed([4, 5].as_slice())));
        assert_eq!(view.borrow(2), None);
    }

    #[test]
    fn wrapping_slice() {
        let mut arr = [0; 10];
        let mut reader = setup(&mut arr, 7);

        arr[3] += 8; // Advance 2 commands

        let mut view = reader.view();
        assert_eq!(view.borrow(6), None);
        assert_eq!(view.borrow(2), Some(RefOrBox::Refed([3, 4].as_slice())));
        view.commit();

        let mut view = reader.view();
        assert_eq!(view.borrow(3), Some(RefOrBox::Refed([5, 6, 1].as_slice())));
        assert_eq!(view.borrow(2), None);
        view.commit();

        let view = reader.view();
        assert_eq!(view.available(), 0);

        // 2nd test
        reader = setup(&mut arr, 7);

        arr[3] += 8; // Advance 2 commands

        let mut view = reader.view();
        assert_eq!(view.borrow(6), None);
        assert_eq!(view.borrow(4), Some(RefOrBox::Refed([3, 4, 5, 6].as_slice())));
        view.commit();

        let mut view = reader.view();
        assert_eq!(view.borrow(1), Some(RefOrBox::Refed([1].as_slice())));
        assert_eq!(view.borrow(1), None);
        view.commit();
    }
}
