//! Events, dispatch, pointer tracking, and focus traversal (02 §6).
//!
//! Dispatch is DOM-style — capture (root→target), target, then bubble
//! (target→root) — routed over the SoA hit-test, never via widget-trait calls.
//! There is exactly one input path: OS and synthesized input share the same
//! [`InputQueue`], so tests and the agent drive the app identically.
//!
//! Consumed by the headless `App` in T0.9; `allow(dead_code)` is removed then.

use crate::tree::{NodeFlags, Tree};
use crate::NodeIndex;
use kurbo::{Point, Size, Vec2};
use smol_str::SmolStr;

bitflags::bitflags! {
    /// Keyboard/pointer modifier state.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub struct Modifiers: u8 {
        /// Shift.
        const SHIFT = 1 << 0;
        /// Control.
        const CTRL  = 1 << 1;
        /// Alt / Option.
        const ALT   = 1 << 2;
        /// Meta / Command / Windows.
        const META  = 1 << 3;
    }
}

/// Which pointer button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PointerButton {
    /// Primary (left).
    Left,
    /// Secondary (right).
    Right,
    /// Middle.
    Middle,
    /// Another button by index.
    Other(u16),
}

/// What kind of pointer produced an event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PointerKind {
    /// Mouse.
    Mouse,
    /// Touch.
    Touch,
    /// Pen / stylus.
    Pen,
}

/// A pointer event payload.
#[derive(Clone, Copy, Debug)]
pub struct PointerEvent {
    /// Window-space position.
    pub pos: Point,
    /// Button involved (for down/up).
    pub button: PointerButton,
    /// Pointer kind.
    pub pointer: PointerKind,
    /// Modifier state.
    pub modifiers: Modifiers,
    /// Click count (1 = single, 2 = double, …).
    pub click_count: u8,
}

impl PointerEvent {
    /// A simple mouse event at `pos` with no modifiers.
    pub fn at(pos: Point) -> PointerEvent {
        PointerEvent {
            pos,
            button: PointerButton::Left,
            pointer: PointerKind::Mouse,
            modifiers: Modifiers::empty(),
            click_count: 1,
        }
    }
}

/// A scroll-wheel event.
#[derive(Clone, Copy, Debug)]
pub struct WheelEvent {
    /// Window-space position.
    pub pos: Point,
    /// Scroll delta (logical px).
    pub delta: Vec2,
    /// Modifier state.
    pub modifiers: Modifiers,
}

/// A named (non-character) key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NamedKey {
    /// Tab.
    Tab,
    /// Enter / Return.
    Enter,
    /// Escape.
    Escape,
    /// Space.
    Space,
    /// Backspace.
    Backspace,
    /// Delete.
    Delete,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Home.
    Home,
    /// End.
    End,
}

/// A key identity: a named key or a produced character.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Key {
    /// A named key.
    Named(NamedKey),
    /// A character-producing key.
    Character(SmolStr),
}

/// A keyboard event.
#[derive(Clone, Debug)]
pub struct KeyEvent {
    /// The key.
    pub key: Key,
    /// Modifier state.
    pub modifiers: Modifiers,
    /// Whether this is an auto-repeat.
    pub repeat: bool,
}

/// Committed text (post-IME).
#[derive(Clone, Debug)]
pub struct TextInputEvent {
    /// The committed text.
    pub text: String,
}

/// IME pre-edit (composition) state.
#[derive(Clone, Debug)]
pub struct ImeEvent {
    /// The current pre-edit string.
    pub preedit: String,
    /// Optional selection within the pre-edit, as `(start, end)` byte offsets.
    pub cursor: Option<(usize, usize)>,
}

/// The active theme variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeKind {
    /// Light theme.
    Light,
    /// Dark theme.
    Dark,
    /// High-contrast theme.
    HighContrast,
}

/// An opaque timer identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TimerToken(pub u64);

/// A high-level gesture recognized from raw touch input (02 §6, 03 §94).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureEvent {
    /// Gesture kind, carrying its parameters.
    pub kind: GestureKind,
    /// Centroid position (mean of active touch points).
    pub pos: Point,
    /// Number of touch points involved.
    pub pointers: u8,
}

/// Gesture kinds with their parameters.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum GestureKind {
    /// A single tap.
    Tap,
    /// Two taps in quick succession.
    DoubleTap,
    /// A press held past the long-press threshold without moving.
    LongPress,
    /// A single-pointer drag.
    Pan {
        /// Movement since the previous pan event.
        delta: Vec2,
        /// Instantaneous velocity (logical px/s).
        velocity: Vec2,
    },
    /// A two-pointer scale gesture.
    Pinch {
        /// Current distance / initial distance between the two pointers.
        scale: f64,
        /// Rate of change of `scale` (per second).
        velocity: f64,
    },
}

/// Application-defined event payload.
pub trait AnyEvent: std::fmt::Debug + 'static {}

/// An input or lifecycle event (02 §6).
#[derive(Debug)]
pub enum Event {
    /// Pointer button pressed.
    PointerDown(PointerEvent),
    /// Pointer button released.
    PointerUp(PointerEvent),
    /// Pointer moved.
    PointerMove(PointerEvent),
    /// Pointer entered a node.
    PointerEnter(PointerEvent),
    /// Pointer left a node.
    PointerLeave(PointerEvent),
    /// Scroll wheel.
    Wheel(WheelEvent),
    /// Key pressed.
    KeyDown(KeyEvent),
    /// Key released.
    KeyUp(KeyEvent),
    /// Committed text (post-IME).
    TextInput(TextInputEvent),
    /// IME pre-edit.
    ImePreedit(ImeEvent),
    /// Node gained focus.
    FocusIn,
    /// Node lost focus.
    FocusOut,
    /// Window resized.
    WindowResized(Size),
    /// Theme changed.
    ThemeChanged(ThemeKind),
    /// Timer fired.
    Timer(TimerToken),
    /// High-level gesture.
    Gesture(GestureEvent),
    /// A drag-and-drop payload dropped at a position (T5.2).
    Drop(DropEvent),
    /// Application-defined.
    Custom(Box<dyn AnyEvent>),
}

/// A drag-and-drop drop at a window-space position.
#[derive(Clone, Debug)]
pub struct DropEvent {
    /// Drop position.
    pub pos: Point,
    /// Dropped payload.
    pub data: DropData,
}

/// Drag-and-drop payload — text and/or file paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DropData {
    /// Dropped text, if any.
    pub text: Option<String>,
    /// Dropped file paths, if any.
    pub files: Vec<String>,
}

/// Whether a handler consumed an event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventStatus {
    /// Consumed; stop propagation.
    Handled,
    /// Not consumed; continue propagation.
    Ignored,
}

/// Dispatch phase for a node along the target path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Capturing, root → target's parent.
    Capture,
    /// At the target node.
    Target,
    /// Bubbling, target's parent → root.
    Bubble,
}

/// The single input queue shared by OS and synthesized input (02 §6).
#[derive(Default)]
pub struct InputQueue {
    queue: std::collections::VecDeque<Event>,
}

impl InputQueue {
    /// A new empty queue.
    pub fn new() -> InputQueue {
        InputQueue::default()
    }
    /// Enqueue an event (OS or synthesized — the same path).
    pub fn push(&mut self, ev: Event) {
        self.queue.push_back(ev);
    }
    /// Pop the next event.
    pub fn pop(&mut self) -> Option<Event> {
        self.queue.pop_front()
    }
    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// The root→target node path (using parent links).
pub fn path_to(tree: &Tree, target: NodeIndex) -> Vec<NodeIndex> {
    let mut path = Vec::new();
    let mut n = target;
    while n.is_some() {
        path.push(n);
        n = tree.parent(n);
    }
    path.reverse();
    path
}

/// Dispatch `event` to `target` with capture → target → bubble phases, invoking
/// `visit` per node. Returns `Handled` if any handler consumed it; the first
/// `Handled` stops all further propagation.
pub fn dispatch(
    tree: &Tree,
    target: NodeIndex,
    event: &Event,
    mut visit: impl FnMut(NodeIndex, Phase, &Event) -> EventStatus,
) -> EventStatus {
    let path = path_to(tree, target);
    if path.is_empty() {
        return EventStatus::Ignored;
    }
    let last = path.len() - 1;
    // capture: root → target's parent
    for &n in &path[..last] {
        if visit(n, Phase::Capture, event) == EventStatus::Handled {
            return EventStatus::Handled;
        }
    }
    // target
    if visit(path[last], Phase::Target, event) == EventStatus::Handled {
        return EventStatus::Handled;
    }
    // bubble: target's parent → root
    for &n in path[..last].iter().rev() {
        if visit(n, Phase::Bubble, event) == EventStatus::Handled {
            return EventStatus::Handled;
        }
    }
    EventStatus::Ignored
}

/// Tracks the hovered path and computes enter/leave transitions on pointer move.
#[derive(Default)]
pub struct PointerState {
    hovered: Vec<NodeIndex>,
}

impl PointerState {
    /// A fresh state with nothing hovered.
    pub fn new() -> PointerState {
        PointerState::default()
    }

    /// Update the hovered path to the node under `pos` (or nothing). Returns
    /// `(leaves, enters)`: nodes to receive `PointerLeave` (deepest first) and
    /// `PointerEnter` (shallowest first).
    pub fn update(&mut self, tree: &Tree, pos: Point) -> (Vec<NodeIndex>, Vec<NodeIndex>) {
        let new_path = match tree.hit_test(pos) {
            Some(t) => path_to(tree, t),
            None => Vec::new(),
        };
        let leaves: Vec<NodeIndex> = self
            .hovered
            .iter()
            .rev()
            .filter(|n| !new_path.contains(n))
            .copied()
            .collect();
        let enters: Vec<NodeIndex> = new_path
            .iter()
            .filter(|n| !self.hovered.contains(n))
            .copied()
            .collect();
        self.hovered = new_path;
        (leaves, enters)
    }

    /// The current hovered (topmost) node, if any.
    pub fn hovered(&self) -> Option<NodeIndex> {
        self.hovered.last().copied()
    }
}

/// All focusable nodes in document order.
pub fn focus_ring(tree: &Tree) -> Vec<NodeIndex> {
    tree.document_order()
        .into_iter()
        .filter(|&n| tree.flags(n).contains(NodeFlags::FOCUSABLE))
        .collect()
}

/// The next focusable node after `current` in document order (`forward` =
/// Tab, else Shift+Tab), wrapping around. With no current focus, returns the
/// first (or last) focusable node.
pub fn next_focus(tree: &Tree, current: Option<NodeIndex>, forward: bool) -> Option<NodeIndex> {
    let ring = focus_ring(tree);
    if ring.is_empty() {
        return None;
    }
    let idx = current.and_then(|c| ring.iter().position(|&n| n == c));
    let next = match idx {
        Some(i) => {
            if forward {
                (i + 1) % ring.len()
            } else {
                (i + ring.len() - 1) % ring.len()
            }
        }
        None => {
            if forward {
                0
            } else {
                ring.len() - 1
            }
        }
    };
    Some(ring[next])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{NodeFlags, Tree};
    use kurbo::Rect;

    fn hittable(t: &mut Tree, n: NodeIndex, r: Rect) {
        t.set_flags(n, NodeFlags::VISIBLE | NodeFlags::HIT_TESTABLE);
        t.set_bounds(n, r);
    }

    #[test]
    fn dispatch_capture_target_bubble_order() {
        let mut t = Tree::new();
        let root = t.insert_root();
        let a = t.insert_child(root);
        let b = t.insert_child(a);
        let mut log = Vec::new();
        let status = dispatch(&t, b, &Event::FocusIn, |n, phase, _| {
            log.push((n, phase));
            EventStatus::Ignored
        });
        assert_eq!(status, EventStatus::Ignored);
        assert_eq!(
            log,
            vec![
                (root, Phase::Capture),
                (a, Phase::Capture),
                (b, Phase::Target),
                (a, Phase::Bubble),
                (root, Phase::Bubble),
            ]
        );
    }

    #[test]
    fn handled_stops_propagation() {
        let mut t = Tree::new();
        let root = t.insert_root();
        let a = t.insert_child(root);
        let b = t.insert_child(a);
        let mut log = Vec::new();
        let status = dispatch(&t, b, &Event::FocusIn, |n, phase, _| {
            log.push((n, phase));
            if n == a && phase == Phase::Capture {
                EventStatus::Handled
            } else {
                EventStatus::Ignored
            }
        });
        assert_eq!(status, EventStatus::Handled);
        assert_eq!(log, vec![(root, Phase::Capture), (a, Phase::Capture)]);
    }

    #[test]
    fn pointer_enter_leave_on_moves() {
        // root contains a (left) and c (right); a contains b.
        let mut t = Tree::new();
        let root = t.insert_root();
        hittable(&mut t, root, Rect::new(0.0, 0.0, 200.0, 100.0));
        let a = t.insert_child(root);
        hittable(&mut t, a, Rect::new(0.0, 0.0, 100.0, 100.0));
        let b = t.insert_child(a);
        hittable(&mut t, b, Rect::new(10.0, 10.0, 90.0, 90.0));
        let c = t.insert_child(root);
        hittable(&mut t, c, Rect::new(100.0, 0.0, 200.0, 100.0));

        let mut ps = PointerState::new();
        // move into b: enter root, a, b
        let (leaves, enters) = ps.update(&t, Point::new(50.0, 50.0));
        assert!(leaves.is_empty());
        assert_eq!(enters, vec![root, a, b]);
        // move into c: leave b, a (deepest first); enter c
        let (leaves, enters) = ps.update(&t, Point::new(150.0, 50.0));
        assert_eq!(leaves, vec![b, a]);
        assert_eq!(enters, vec![c]);
        assert_eq!(ps.hovered(), Some(c));
        // move outside everything
        let (leaves, enters) = ps.update(&t, Point::new(500.0, 500.0));
        assert_eq!(leaves, vec![c, root]);
        assert!(enters.is_empty());
    }

    #[test]
    fn focus_ring_traverses_in_document_order() {
        // 20-node fixture: mark every 3rd node focusable.
        let mut t = Tree::new();
        let root = t.insert_root();
        let mut nodes = vec![root];
        for _ in 0..19 {
            let n = t.insert_child(root);
            nodes.push(n);
        }
        let mut expected = Vec::new();
        for (i, &n) in nodes.iter().enumerate() {
            if i % 3 == 0 {
                t.set_flags(n, NodeFlags::VISIBLE | NodeFlags::FOCUSABLE);
                expected.push(n);
            }
        }
        assert_eq!(focus_ring(&t), expected);

        // Tab through the whole ring and wrap.
        let mut cur = None;
        let mut visited = Vec::new();
        for _ in 0..expected.len() {
            cur = next_focus(&t, cur, true);
            visited.push(cur.unwrap());
        }
        assert_eq!(visited, expected);
        // wrap-around
        assert_eq!(
            next_focus(&t, Some(*expected.last().unwrap()), true),
            Some(expected[0])
        );
        // Shift+Tab from the first goes to the last
        assert_eq!(
            next_focus(&t, Some(expected[0]), false),
            Some(*expected.last().unwrap())
        );
    }
}
