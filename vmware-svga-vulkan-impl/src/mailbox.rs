use std::sync::atomic::Ordering::*;
use std::{
    ops::{Deref, DerefMut},
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};

use log::debug;
use parking_lot::{RwLock, RwLockWriteGuard};

type MailboxItem = Option<Vec<u32>>;

/** "Mailbox" style swapchain */
pub struct Mailbox {
    latest: AtomicUsize,
    arr: [RwLock<MailboxItem>; 3],
}

struct MailboxWriter<'a> {
    obj: Arc<Mailbox>,
    index: usize,
    target: RwLockWriteGuard<'a, MailboxItem>,
}

impl Deref for MailboxWriter<'_> {
    type Target = MailboxItem;

    fn deref(&self) -> &Self::Target {
        &self.target
    }
}

impl DerefMut for MailboxWriter<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.target
    }
}

impl Drop for MailboxWriter<'_> {
    fn drop(&mut self) {
        self.obj.latest.store(self.index, Relaxed);
    }
}

impl Mailbox {
    pub fn new() -> Arc<Self> {
        Mailbox {
            latest: AtomicUsize::new(0),
            arr: Default::default(),
        }
        .into()
    }

    pub fn borrow_write<'a>(self: &'a Arc<Self>) -> impl DerefMut<Target = MailboxItem> + 'a {
        let latest = self.latest.load(Relaxed);

        for i in 1..(self.arr.len()) {
            let idx = (latest + i) % self.arr.len();
            if let Some(x) = self.arr[idx].try_write() {
                return MailboxWriter {
                    obj: Arc::clone(self),
                    index: idx,
                    target: x,
                };
            }
        }

        // Could not find lockable entry
        let mut iter_cnt = 0;

        loop {
            debug!("Writer waiting for entry - {iter_cnt}s...");
            iter_cnt += 1;

            let idx = (self.latest.load(Relaxed) + 1) % self.arr.len();
            if let Some(x) = self.arr[idx].try_write_for(Duration::from_secs(1)) {
                return MailboxWriter {
                    obj: Arc::clone(self),
                    index: idx,
                    target: x,
                };
            }
        }
    }

    pub fn borrow_read(&self) -> impl Deref<Target = MailboxItem> + '_ {
        let latest = self.latest.load(Relaxed);
        self.arr[latest].read()
    }
}
