//! A bounded single-producer single-consumer ring, `std` only.
//!
//! The game thread pushes commands, the audio thread pops them: no locks, no allocation after
//! [`channel`], wait-free on both sides. Each side owns one index and only reads the other's, so
//! a push is one `Acquire` load, a write and one `Release` store.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Keeps the two indices on separate cache lines so the threads don't false-share.
#[repr(align(64))]
struct Padded(AtomicUsize);

struct Inner<T> {
    /// Next slot the consumer reads (only the consumer writes it).
    head: Padded,
    /// Next slot the producer writes (only the producer writes it).
    tail: Padded,
    mask: usize,
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
}

// SAFETY: slots are handed from exactly one producer to exactly one consumer through the
// Release/Acquire index protocol, so a `T` is only ever accessed by one thread at a time; that is
// sound whenever `T: Send`.
unsafe impl<T: Send> Sync for Inner<T> {}
// SAFETY: as above; the ring owns its `T`s.
unsafe impl<T: Send> Send for Inner<T> {}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        let mut head = *self.head.0.get_mut();
        let tail = *self.tail.0.get_mut();
        while head != tail {
            // SAFETY: slots in head..tail were written by the producer and never read.
            unsafe { (*self.slots[head & self.mask].get()).assume_init_drop() };
            head = head.wrapping_add(1);
        }
    }
}

/// The sending half. Not `Clone`: there is exactly one producer.
pub struct Producer<T> {
    inner: Arc<Inner<T>>,
}

/// The receiving half. Not `Clone`: there is exactly one consumer.
pub struct Consumer<T> {
    inner: Arc<Inner<T>>,
}

/// A ring holding at least `capacity` items (rounded up to a power of two, minimum 2).
pub fn channel<T: Send>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let cap = capacity.max(2).next_power_of_two();
    let slots = (0..cap).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect::<Vec<_>>().into_boxed_slice();
    let inner = Arc::new(Inner { head: Padded(AtomicUsize::new(0)), tail: Padded(AtomicUsize::new(0)), mask: cap - 1, slots });
    (Producer { inner: inner.clone() }, Consumer { inner })
}

impl<T: Send> Producer<T> {
    /// Pushes `value`, or hands it back if the ring is full.
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let inner = &*self.inner;
        let tail = inner.tail.0.load(Ordering::Relaxed);
        let head = inner.head.0.load(Ordering::Acquire);
        if tail.wrapping_sub(head) > inner.mask {
            return Err(value);
        }
        // SAFETY: the slot at `tail` is outside head..tail, so the consumer isn't touching it, and
        // only this (unique) producer writes slots. The Release store below publishes the write.
        unsafe { (*inner.slots[tail & inner.mask].get()).write(value) };
        inner.tail.0.store(tail.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Items currently queued (a snapshot).
    pub fn len(&self) -> usize {
        let tail = self.inner.tail.0.load(Ordering::Relaxed);
        tail.wrapping_sub(self.inner.head.0.load(Ordering::Acquire))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.inner.mask + 1
    }
}

impl<T: Send> Consumer<T> {
    /// Pops the oldest item, if any.
    pub fn pop(&mut self) -> Option<T> {
        let inner = &*self.inner;
        let head = inner.head.0.load(Ordering::Relaxed);
        let tail = inner.tail.0.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        // SAFETY: `head` is in head..tail, so the producer finished writing it (Acquire above pairs
        // with its Release) and won't touch it until we advance `head`. Reading moves the value
        // out exactly once.
        let value = unsafe { (*inner.slots[head & inner.mask].get()).assume_init_read() };
        inner.head.0.store(head.wrapping_add(1), Ordering::Release);
        Some(value)
    }

    pub fn len(&self) -> usize {
        let head = self.inner.head.0.load(Ordering::Relaxed);
        self.inner.tail.0.load(Ordering::Acquire).wrapping_sub(head)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_and_full() {
        let (mut p, mut c) = channel::<u32>(3);
        assert_eq!(p.capacity(), 4);
        for i in 0..4 {
            p.push(i).unwrap();
        }
        assert_eq!(p.push(9), Err(9));
        assert_eq!(c.pop(), Some(0));
        p.push(4).unwrap();
        let got: Vec<u32> = std::iter::from_fn(|| c.pop()).collect();
        assert_eq!(got, vec![1, 2, 3, 4]);
        assert!(c.is_empty());
    }

    #[test]
    fn drops_unread_items() {
        let marker = Arc::new(());
        {
            let (mut p, _c) = channel::<Arc<()>>(8);
            for _ in 0..5 {
                p.push(marker.clone()).unwrap();
            }
            assert_eq!(Arc::strong_count(&marker), 6);
        }
        assert_eq!(Arc::strong_count(&marker), 1);
    }

    #[test]
    fn threads_preserve_order_and_count() {
        const N: u64 = 200_000;
        let (mut p, mut c) = channel::<u64>(64);
        let producer = std::thread::spawn(move || {
            let mut i = 0;
            while i < N {
                match p.push(i) {
                    Ok(()) => i += 1,
                    Err(_) => std::thread::yield_now(),
                }
            }
        });
        let mut expected = 0;
        let mut sum = 0u64;
        while expected < N {
            if let Some(v) = c.pop() {
                assert_eq!(v, expected, "out of order");
                sum += v;
                expected += 1;
            } else {
                std::thread::yield_now();
            }
        }
        producer.join().unwrap();
        assert_eq!(sum, N * (N - 1) / 2);
        assert!(c.pop().is_none());
    }
}
