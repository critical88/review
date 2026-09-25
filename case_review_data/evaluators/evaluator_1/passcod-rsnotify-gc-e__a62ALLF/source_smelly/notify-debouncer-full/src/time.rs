//! Mockable clock for the debouncer tests.
//!
//! This module is only compiled for tests: the production clock is owned by
//! [`crate::Debouncer`], and tests place it under the [`MockTime`] control
//! implemented here so state-machine tests can fast-forward time.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

thread_local! {
    static NOW: Mutex<Option<Instant>> = const { Mutex::new(None) };
}

/// The current test clock reading.
pub fn now() -> Instant {
    let time = NOW.with(|now| *now.lock().unwrap());
    time.unwrap_or_else(Instant::now)
}

pub struct MockTime;

impl MockTime {
    pub fn set_time(time: Instant) {
        NOW.with(|now| *now.lock().unwrap() = Some(time));
    }

    pub fn advance(delta: Duration) {
        NOW.with(|now| {
            if let Some(n) = &mut *now.lock().unwrap() {
                *n += delta;
            }
        });
    }
}
