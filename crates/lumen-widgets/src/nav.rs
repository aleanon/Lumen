//! Navigation & global state (T5.4). A [`Router`] is a serializable back-stack
//! value held in a signal, so navigation state persists through the Checkpoint
//! protocol (tier-3 restart / save-load) like any other state. Deep links and
//! guards are plain methods.

use serde::{Deserialize, Serialize};

/// A navigation back-stack. The current route is the top of the stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Router {
    stack: Vec<String>,
}

impl Router {
    /// A router rooted at `root`.
    pub fn new(root: impl Into<String>) -> Router {
        Router {
            stack: vec![root.into()],
        }
    }

    /// The current (top) route.
    pub fn current(&self) -> &str {
        self.stack.last().map(String::as_str).unwrap_or("/")
    }

    /// Push a new route.
    pub fn navigate(&mut self, route: impl Into<String>) {
        self.stack.push(route.into());
    }

    /// Pop the top route; returns whether it moved (false at the root).
    pub fn back(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            true
        } else {
            false
        }
    }

    /// Navigate only if `guard(route)` allows it; returns whether it navigated.
    pub fn navigate_guarded(
        &mut self,
        route: impl Into<String>,
        guard: impl Fn(&str) -> bool,
    ) -> bool {
        let route = route.into();
        if guard(&route) {
            self.stack.push(route);
            true
        } else {
            false
        }
    }

    /// Replace the stack from a deep-link path like `"settings/appearance"`.
    pub fn deep_link(&mut self, path: &str) {
        self.stack = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if self.stack.is_empty() {
            self.stack.push("/".to_string());
        }
    }

    /// Whether `back` would move.
    pub fn can_go_back(&self) -> bool {
        self.stack.len() > 1
    }

    /// Back-stack depth.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}
