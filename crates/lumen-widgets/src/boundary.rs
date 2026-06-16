//! Error boundaries (T7.3): contain a panic in a UI subtree's build so the rest
//! of the app keeps running, rendering a fallback instead of crashing the
//! process — like React error boundaries, but for native Rust UI.

use crate::widgets;
use crate::Element;

/// Build `child`; if it panics, catch the unwind and render `fallback(message)`
/// instead. The panic never propagates past this node, so sibling subtrees and
/// the app keep running.
pub fn error_boundary<F, G>(child: F, fallback: G) -> Element
where
    F: FnOnce() -> Element,
    G: FnOnce(String) -> Element,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(child)) {
        Ok(el) => el,
        Err(payload) => fallback(panic_message(&payload)),
    }
}

/// A default fallback: a labelled error node carrying the message.
pub fn default_fallback(message: String) -> Element {
    widgets::text(format!("⚠ {message}")).id("error-boundary")
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic".to_string()
    }
}
