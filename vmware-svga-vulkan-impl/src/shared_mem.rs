use std::{
    marker::PhantomData,
    mem::{align_of, size_of},
    ptr::{null_mut, slice_from_raw_parts},
    sync::atomic::{AtomicU16, AtomicU32, AtomicU8},
};

use bytemuck::Pod;

#[derive(Clone)]
pub struct SharedMem<T: Copy + Pod> {
    ptr: *mut u8,
    len: usize,
    _phantom: PhantomData<T>,
}

unsafe impl<T: Copy + Pod> Send for SharedMem<T> {}
unsafe impl<T: Copy + Pod> Sync for SharedMem<T> {}

impl<T: Copy + Pod> SharedMem<T> {
    pub fn new(ptr: *mut u8, len: usize) -> Self {
        //TODO: This probably needs to be unsafe
        assert_ne!(ptr, null_mut(), "Received null pointer");
        assert_eq!(
            len % size_of::<T>(),
            0,
            "Length={len} not divisible by {}",
            size_of::<T>()
        );
        assert_eq!(
            ptr as usize % align_of::<T>(),
            0,
            "Pointer not aligned to {}",
            align_of::<T>()
        );
        SharedMem {
            ptr,
            len,
            _phantom: Default::default(),
        }
    }

    pub fn byte_at(&self, byte_offset: usize) -> &AtomicU8 {
        assert!(byte_offset < self.len, "buffer overflow");
        unsafe { &*(self.ptr.add(byte_offset) as *const std::sync::atomic::AtomicU8) }
    }

    pub fn read_volatile(&self, offset: usize) -> T {
        assert!(offset * size_of::<T>() < self.len, "buffer overflow");
        let p = self.ptr as *mut T;
        unsafe { p.add(offset).read_volatile() }
    }

    pub fn write_volatile(&self, offset: usize, val: T) {
        assert!(offset * size_of::<T>() < self.len, "buffer overflow");
        let p = self.ptr as *mut T;
        unsafe {
            p.add(offset).write_volatile(val);
        }
    }

    pub fn slice_to(&self, begin: usize, end: usize) -> &[T] {
        assert!(begin <= end, "inverted range");
        assert!(end * size_of::<T>() <= self.len, "buffer overflow");
        let len = end - begin;
        let p = self.ptr as *mut T;
        unsafe { &*slice_from_raw_parts(p.add(begin), len) }
    }

    pub fn as_ptr(&self) -> *mut T {
        self.ptr as *mut T
    }

    pub fn as_byte_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl SharedMem<u8> {
    pub fn at(&self, offset: usize) -> &AtomicU8 {
        assert!(offset < self.len, "buffer overflow");
        unsafe { &*(self.ptr.add(offset) as *const std::sync::atomic::AtomicU8) }
    }
}

impl SharedMem<u16> {
    pub fn at(&self, offset: usize) -> &AtomicU16 {
        assert!(offset * 2 < self.len, "buffer overflow");
        unsafe { &*(self.ptr.add(offset * 2) as *const std::sync::atomic::AtomicU16) }
    }
}

impl SharedMem<u32> {
    pub fn at(&self, offset: usize) -> &AtomicU32 {
        assert!(offset * 4 < self.len, "buffer overflow");
        unsafe { &*(self.ptr.add(offset * 4) as *const std::sync::atomic::AtomicU32) }
    }
}
