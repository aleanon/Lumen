//! A minimal single-threaded executor for test bodies.
//!
//! The harness's `async fn`s never suspend on real I/O — auto-wait loops drive
//! the synchronous `Headless::pump` directly — so a body completes on the first
//! poll. This keeps `tokio` out of the test harness (ADR-003 scopes it to the
//! agent/dev-server). See decision log (T0.9).

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

/// Block on `future` to completion using a no-op waker.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
}
