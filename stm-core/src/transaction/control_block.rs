// Copyright 2015-2016 rust-stm Developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, Thread};
use std::time::Duration;

#[cfg(test)]
use super::super::test::{terminates, terminates_async};

/// A control block for a currently running STM instance.
///
/// STM blocks on all read variables if retry was called.
/// This control block is used to let the vars inform the STM instance.
///
/// Be careful when using this directly,
/// because you can easily create deadlocks.
pub struct ControlBlock {
    /// This is the handle to the thread, that waits on the control block.
    thread: Thread,

    /// Atomic bool stores if the thread has been blocked yet.
    /// Make sure, that park is repeated if no change has happened.
    blocked: AtomicBool,

    // Safety check to avoid deadlocks.
    park_timeout: Duration,
}

impl ControlBlock {
    #[cfg_attr(feature = "cargo-clippy", allow(new_without_default_derive))]

    /// Create a new StmControlBlock.
    pub fn new() -> ControlBlock {
        ControlBlock {
            thread: thread::current(),
            blocked: AtomicBool::new(true),
            park_timeout: Duration::from_millis(1000),
        }
    }

    /// Inform the control block that a variable has changed.
    ///
    /// Need to be called from outside of STM.
    pub fn set_changed(&self) {
        // Only wakeup once.
        if self.blocked.swap(false, Ordering::SeqCst) {
            // wake thread
            self.thread.unpark();
        }
    }

    /// Block until one variable has changed.
    ///
    /// `wait` may immediately return.
    ///
    /// `wait` needs to be called by the STM instance itself.
    pub fn wait(&self) {
        while self.blocked.load(Ordering::SeqCst) {
            // Deadlocks can happen when `set_change` runs here,
            // or we call `wait` on a thread which is different
            // from `self.thread`.

            // assert_eq!(thread::current().id(), self.thread.id());
            // thread::park();

            // To deal with both, make sure the thread is not parked forever.
            thread::park_timeout(self.park_timeout);
        }
    }

    /// Here to make tests faster while allowing a long timeout in the normal case.
    #[allow(dead_code)]
    fn set_park_timeout(&mut self, park_timeout: Duration) {
        self.park_timeout = park_timeout;
    }
}

// TESTS
#[cfg(test)]
mod test {
    use super::*;

    /// Test if `ControlBlock` correctly blocks on `wait`.
    #[test]
    fn blocked() {
        let ctrl = ControlBlock::new();
        // waiting should immediately finish
        assert!(!terminates(100, move || ctrl.wait()));
    }

    /// A `ControlBlock` does immediately return,
    /// when it was set to changed before calling waiting.
    ///
    /// This scenario may occur, when a variable changes, while the
    /// transaction has not yet blocked.
    #[test]
    fn wait_after_change() {
        let ctrl = ControlBlock::new();
        // set to changed
        ctrl.set_changed();
        // waiting should immediately finish
        assert!(terminates(50, move || ctrl.wait()));
    }

    /// Test calling `set_changed` multiple times.
    #[test]
    fn wait_after_multiple_changes() {
        let ctrl = ControlBlock::new();
        // set to changed
        ctrl.set_changed();
        ctrl.set_changed();
        ctrl.set_changed();
        ctrl.set_changed();

        // waiting should immediately finish
        assert!(terminates(50, move || ctrl.wait()));
    }

    /// Perform a wakeup from another thread.
    #[test]
    fn wait_threaded_wakeup() {
        use std::sync::Arc;

        let ctrl = Arc::new({
            let mut ctrl = ControlBlock::new();
            ctrl.set_park_timeout(Duration::from_millis(250));
            ctrl
        });
        let ctrl2 = ctrl.clone();
        // NOTE: This is slightly broken: `terminates_async` will run
        // `f` on a spawned thread and `g` on the main thread, which means
        // the thread that parks itself in `wait` will be different from the
        // one that `set_changes` wakes up, which is the main thread itself.
        // That means `f` will never be unparked, unless it uses a timeout;
        // `wait` should only really be called on the thread that did `new`.
        let terminated = terminates_async(
            500,
            move || ctrl.wait(),
            move || {
                thread::sleep(Duration::from_millis(100));
                ctrl2.set_changed();
            },
        );

        assert!(terminated);
    }
}
