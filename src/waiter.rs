use std::sync::atomic::Ordering;
use std::time::Duration;
use std::{fmt, io};

use may::coroutine;
use may::sync::{AtomicOption, Blocker};

pub struct Waiter<T> {
    blocker: Blocker,
    rsp: AtomicOption<T>,
}

impl<T> Waiter<T> {
    pub fn new() -> Self {
        Waiter {
            blocker: Blocker::new(false),
            rsp: AtomicOption::none(),
        }
    }

    pub fn set_rsp(&self, rsp: T) {
        // set the response
        self.rsp.swap(rsp, Ordering::Release);
        // wake up the blocker
        self.blocker.unpark();
    }

    pub fn wait_rsp<D: Into<Option<Duration>>>(&self, timeout: D) -> io::Result<T> {
        use coroutine::ParkError;
        use io::{Error, ErrorKind};
        let timeout = timeout.into();
        loop {
            match self.blocker.park(timeout) {
                Ok(_) => match self.rsp.take(Ordering::Acquire) {
                    Some(rsp) => return Ok(rsp),
                    // None => Err(Error::new(ErrorKind::Other, "unable to get the rsp")),
                    // false wakeup try again
                    None => {}
                },
                Err(ParkError::Timeout) => {
                    return Err(Error::new(ErrorKind::TimedOut, "wait rsp timeout"))
                }
                Err(ParkError::Canceled) => {
                    coroutine::trigger_cancel_panic();
                }
            }
        }
    }
}

impl<T> fmt::Debug for Waiter<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Waiter{{ ... }}")
    }
}

impl<T> Default for Waiter<T> {
    fn default() -> Self {
        Waiter::new()
    }
}
