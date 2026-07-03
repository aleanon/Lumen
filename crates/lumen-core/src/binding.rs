//! Reactive property bindings (F3, `docs/plan-fine-grained-view.md`).
//!
//! Option B: the view is built once and a *dynamic* prop is a **binding** — a
//! closure over the store that re-runs only when the signals it read change,
//! patching that one prop rather than re-running the build. [`Dynamic<T>`] is the
//! closure; [`Prop<T>`] is "static value **or** binding" for an element field.
//!
//! A binding is *declared* by the author (via the `text!`/`bind!` sugar), so the
//! reactive boundary is explicit and observable — the dependency keys it records
//! on evaluation are exactly what the agent sees (F2 §2 / F4 `getDeps`).

use crate::state::{ReadSet, Runtime};
use std::rc::Rc;

/// A reactively-computed value: a `Fn(&Runtime) -> T` over the store. Cheap to
/// clone (`Rc`). Evaluating it re-reads current signal values; [`Dynamic::eval`]
/// also reports which signals were read (its dependency set).
pub struct Dynamic<T> {
    f: Rc<dyn Fn(&Runtime) -> T>,
}

impl<T> Clone for Dynamic<T> {
    fn clone(&self) -> Self {
        Dynamic { f: self.f.clone() }
    }
}

impl<T> Dynamic<T> {
    /// Wrap a reactive closure. It should read the signals it depends on through
    /// the passed `&Runtime` (`sig.get(rt)`), so evaluation records them.
    pub fn new(f: impl Fn(&Runtime) -> T + 'static) -> Dynamic<T> {
        Dynamic { f: Rc::new(f) }
    }

    /// The current value (untracked; does not record deps).
    pub fn get(&self, rt: &Runtime) -> T {
        (self.f)(rt)
    }

    /// The current value **and** the set of signals it read — the binding's
    /// dependency set, used to decide when to re-evaluate and to report deps.
    pub fn eval(&self, rt: &Runtime) -> (T, ReadSet) {
        rt.collect_reads(|| (self.f)(rt))
    }
}

/// An element property that is either a fixed value or a [`Dynamic`] binding.
/// Defaults to `Static`, so authoring that never binds is unaffected.
pub enum Prop<T> {
    /// A constant value.
    Static(T),
    /// A reactive binding.
    Dynamic(Dynamic<T>),
}

impl<T: Clone> Clone for Prop<T> {
    fn clone(&self) -> Self {
        match self {
            Prop::Static(v) => Prop::Static(v.clone()),
            Prop::Dynamic(d) => Prop::Dynamic(d.clone()),
        }
    }
}

impl<T: Clone> Prop<T> {
    /// The current value (clones a `Static`, evaluates a `Dynamic` untracked).
    pub fn get(&self, rt: &Runtime) -> T {
        match self {
            Prop::Static(v) => v.clone(),
            Prop::Dynamic(d) => d.get(rt),
        }
    }

    /// The current value plus its dependency set — `None` for a `Static` (no
    /// deps), `Some(read_set)` for a binding.
    pub fn eval(&self, rt: &Runtime) -> (T, Option<ReadSet>) {
        match self {
            Prop::Static(v) => (v.clone(), None),
            Prop::Dynamic(d) => {
                let (v, rs) = d.eval(rt);
                (v, Some(rs))
            }
        }
    }

    /// Whether this prop is a binding (vs a constant).
    pub fn is_dynamic(&self) -> bool {
        matches!(self, Prop::Dynamic(_))
    }
}

impl<T> From<T> for Prop<T> {
    fn from(v: T) -> Self {
        Prop::Static(v)
    }
}

impl<T> From<Dynamic<T>> for Prop<T> {
    fn from(d: Dynamic<T>) -> Self {
        Prop::Dynamic(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Signal;

    #[test]
    fn dynamic_evals_current_value_and_records_deps() {
        let rt = Runtime::new();
        let a: Signal<i64> = rt.signal("a", || 2);
        let b: Signal<i64> = rt.signal("b", || 5);
        let rt2 = rt.clone();
        let sum = Dynamic::new(move |rt| a.get(rt) + b.get(rt));

        let (v, reads) = sum.eval(&rt2);
        assert_eq!(v, 7);
        assert!(reads.is_current(&rt), "fresh read set is current");

        // Writing a dep invalidates the set; the value updates.
        a.set(&rt, 10);
        assert!(!reads.is_current(&rt), "a write to a dep is observed");
        assert_eq!(sum.get(&rt), 15);

        // Dep keys are the human signal keys (observability).
        let (_, reads2) = sum.eval(&rt);
        let mut keys = reads2.dep_keys(&rt);
        keys.sort();
        assert_eq!(keys, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn prop_static_and_dynamic() {
        let rt = Runtime::new();
        let n: Signal<i64> = rt.signal("n", || 3);
        let s: Prop<i64> = 42.into();
        assert!(!s.is_dynamic());
        assert_eq!(s.get(&rt), 42);
        assert!(s.eval(&rt).1.is_none());

        let d: Prop<i64> = Dynamic::new(move |rt| n.get(rt) * 2).into();
        assert!(d.is_dynamic());
        assert_eq!(d.get(&rt), 6);
        assert!(d.eval(&rt).1.is_some());
    }
}
