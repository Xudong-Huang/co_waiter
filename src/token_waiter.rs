use std::fmt;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::waiter::Waiter;

pub struct TokenWaiter<T> {
    key: AtomicUsize,
    waiter: Waiter<T>,
}

impl<T> TokenWaiter<T> {
    pub fn new() -> Self {
        TokenWaiter {
            key: AtomicUsize::new(0),
            waiter: Waiter::new(),
        }
    }

    pub fn get_id(self: Pin<&Self>) -> usize {
        let address = self.get_ref() as *const _ as usize;
        let id = address << 3;
        self.key.store(id, Ordering::Relaxed);
        id
    }

    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn from_id(id: &usize) -> Option<&Self> {
        let id = *id;
        // TODO: how to check if the address is valid?
        // if the id is wrong enough we could get a SIGSEGV
        let address = id >> 3;
        if address & 3 != 0 {
            return None;
        }

        let waiter = unsafe { &*(address as *const Self) };
        // need to check if the memory is still valid
        // lock the key to protect contention with drop
        if waiter.key.compare_and_swap(id, id + 1, Ordering::AcqRel) == id {
            Some(waiter)
        } else {
            None
        }
    }

    pub fn wait_rsp<D: Into<Option<Duration>>>(&self, timeout: D) -> io::Result<T> {
        self.waiter.wait_rsp(timeout)
    }

    // set rsp for the waiter with id
    pub fn set_rsp(id: usize, rsp: T) {
        if let Some(waiter) = Self::from_id(&id) {
            // clear the key lock bit
            waiter.key.fetch_and(!1, Ordering::Release);
            // wake up the blocker
            waiter.waiter.set_rsp(rsp);
        }
    }
}

impl<T> fmt::Debug for TokenWaiter<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TokenWaiter{{ ... }}")
    }
}

impl<T> Default for TokenWaiter<T> {
    fn default() -> Self {
        TokenWaiter::new()
    }
}

impl<T> Drop for TokenWaiter<T> {
    fn drop(&mut self) {
        // wait for the key locked and clear it
        let key = self.key.load(Ordering::Relaxed) & !1;
        while self.key.compare_and_swap(key, 0, Ordering::AcqRel) != key {
            std::sync::atomic::spin_loop_hint()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use may::go;

    #[test]
    fn token_waiter() {
        let result = go!(|| {
            let waiter = TokenWaiter::<usize>::new();
            let waiter = Pin::new(&waiter);
            let id = waiter.get_id();
            // trigger the rsp in another coroutine
            go!(move || TokenWaiter::<usize>::set_rsp(id, 42));
            // this will block until the rsp was set
            waiter.wait_rsp(None).unwrap()
        })
        .join()
        .unwrap();

        assert_eq!(result, 42);
    }

    #[test]
    fn token_waiter_timeout() {
        let result = go!(|| {
            let waiter = TokenWaiter::<usize>::new();
            let waiter = Pin::new(&waiter);
            let id = waiter.get_id();
            // trigger the rsp in another coroutine
            let h = go!(move || {
                may::coroutine::sleep(Duration::from_millis(102));
                TokenWaiter::<usize>::set_rsp(id, 42)
            });
            // this will block until the rsp was set
            let ret = waiter.wait_rsp(Duration::from_millis(100));
            h.join().unwrap();
            ret
        })
        .join()
        .unwrap();

        assert_eq!(result.is_err(), true);
    }
}
