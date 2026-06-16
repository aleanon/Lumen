//! Undo / redo (T5.4): a serializable [`History`] of states held in a signal, so
//! it persists like any other state. Each edit pushes the prior value onto the
//! past; `undo`/`redo` move between past and future.

use serde::{Deserialize, Serialize};

/// An undo/redo history over snapshots of a value `T`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History<T> {
    past: Vec<T>,
    present: T,
    future: Vec<T>,
}

impl<T: Clone> History<T> {
    /// A history with the initial present value.
    pub fn new(initial: T) -> History<T> {
        History {
            past: Vec::new(),
            present: initial,
            future: Vec::new(),
        }
    }

    /// The current value.
    pub fn present(&self) -> &T {
        &self.present
    }

    /// Commit a new value (clears the redo stack).
    pub fn push(&mut self, next: T) {
        self.past.push(self.present.clone());
        self.present = next;
        self.future.clear();
    }

    /// Undo one step; returns whether it moved.
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.past.pop() {
            self.future.push(std::mem::replace(&mut self.present, prev));
            true
        } else {
            false
        }
    }

    /// Redo one step; returns whether it moved.
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.future.pop() {
            self.past.push(std::mem::replace(&mut self.present, next));
            true
        } else {
            false
        }
    }

    /// Whether `undo` would move.
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether `redo` would move.
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}
