//! Temporary line flags reused by successive contexts on a worker thread.
//! Only booleans are retained: no source text, paths, or document state.

use std::cell::RefCell;
use std::ops::{Deref, DerefMut};

const MAX_BUFFERS: usize = 8;
const MAX_RETAINED_BYTES: usize = 256 * 1024;

#[derive(Default)]
struct Pool {
    buffers: Vec<Vec<bool>>,
    bytes: usize,
}

thread_local! {
    static FLAGS: RefCell<Pool> = const { RefCell::new(Pool { buffers: Vec::new(), bytes: 0 }) };
}

pub(super) struct LineFlags(Vec<bool>);

impl LineFlags {
    pub(super) fn new(len: usize) -> Self {
        // No borrow stays live while a context is constructed. A nested parse
        // leases another buffer and cannot overwrite a caller's flags.
        let mut flags = FLAGS
            .try_with(|pool| {
                let mut pool = pool.try_borrow_mut().ok()?;
                let flags = pool.buffers.pop()?;
                pool.bytes -= flags.capacity();
                Some(flags)
            })
            .ok()
            .flatten()
            .unwrap_or_default();
        flags.resize(len, false);
        Self(flags)
    }
}

impl Deref for LineFlags {
    type Target = [bool];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LineFlags {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for LineFlags {
    fn drop(&mut self) {
        let mut flags = std::mem::take(&mut self.0);
        flags.clear();
        let capacity = flags.capacity();
        if capacity == 0 || capacity > MAX_RETAINED_BYTES {
            return;
        }
        // Thread-local destructors and unwinding must not introduce a second
        // panic. An unavailable pool simply releases the allocation normally.
        let _ = FLAGS.try_with(|pool| {
            if let Ok(mut pool) = pool.try_borrow_mut()
                && pool.buffers.len() < MAX_BUFFERS
                && pool.bytes + capacity <= MAX_RETAINED_BYTES
            {
                pool.bytes += capacity;
                pool.buffers.push(flags);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_leases_reuse_storage_and_reset_every_flag() {
        FLAGS.with(|pool| *pool.borrow_mut() = Pool::default());
        let mut outer = LineFlags::new(64);
        outer.fill(true);
        let pointer = {
            let mut nested = LineFlags::new(32);
            assert!(nested.iter().all(|flag| !flag));
            nested.fill(true);
            nested.as_ptr()
        };
        let reused = LineFlags::new(16);
        assert_eq!(reused.as_ptr(), pointer);
        assert!(reused.iter().all(|flag| !flag));
        assert!(outer.iter().all(|flag| *flag));
    }

    #[test]
    fn oversized_documents_and_many_nested_parses_bound_retained_storage() {
        FLAGS.with(|pool| *pool.borrow_mut() = Pool::default());
        drop(LineFlags::new(MAX_RETAINED_BYTES + 1));
        FLAGS.with(|pool| assert_eq!(pool.borrow().bytes, 0));
        let flags: Vec<_> = (0..MAX_BUFFERS + 2).map(|_| LineFlags::new(64)).collect();
        drop(flags);
        FLAGS.with(|pool| {
            let pool = pool.borrow();
            assert_eq!(pool.buffers.len(), MAX_BUFFERS);
            assert!(pool.bytes <= MAX_RETAINED_BYTES);
        });
    }
}
