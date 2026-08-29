//! AccessKit integration (T4.3): maps Lumen's semantic [`Role`]/[`State`] onto
//! AccessKit's tree so the same one semantic tree drives platform a11y
//! (VoiceOver / NVDA / AT-SPI) — no separate accessibility pass.
//!
//! The role map is an exhaustive `match`, so adding a Lumen role fails to
//! compile until it is mapped here (the "map table complete" guarantee). See
//! `docs/a11y-checklist.md` for the manual VoiceOver/NVDA verification.

use accesskit::{
    Action as AkAction, ActionData, Node, NodeId, Role as AkRole, ScrollUnit, Toggled, Tree, TreeId,
    TreeUpdate,
};
use kurbo::{Point, Rect, Vec2};
use lumen_core::semantics::{Action, Role, SemanticsNode, State};

/// What the shell should synthesise to satisfy an AT's action request.
///
/// Deliberately *events*, not direct state writes. An AT scroll that bypassed
/// the wheel path would be the one scroll in the app that skips chaining,
/// clamping and momentum, and would drift from what a mouse does the first
/// time any of those changed. Routing through the same events means an AT
/// cannot reach a state a user could not.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AtCommand {
    /// Press and release at this window point.
    Click(Point),
    /// A wheel event at this window point with this delta. Positive `y`
    /// scrolls toward the end, matching `WheelEvent` everywhere else.
    Wheel {
        /// Window-space point to aim the wheel at — the centre of the viewport
        /// being scrolled, not of the node the AT named.
        pos: Point,
        /// Scroll delta in logical px.
        delta: Vec2,
    },
}

fn center(r: Rect) -> Point {
    Point::new((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

/// The chain from `root` down to the node whose published id is `target`.
///
/// A path rather than a bare node because the scroll actions need the target's
/// **ancestors**: `ScrollIntoView` on a list row has to move the list, and the
/// row itself knows nothing about it.
fn path_to<'a>(n: &'a SemanticsNode, target: u64, out: &mut Vec<&'a SemanticsNode>) -> bool {
    out.push(n);
    if n.node.fold64() == target {
        return true;
    }
    for c in &n.children {
        if path_to(c, target, out) {
            return true;
        }
    }
    out.pop();
    false
}

/// How far to scroll a viewport so `what` becomes visible inside `view`.
///
/// Zero on an axis that already contains it — an AT asking to reveal something
/// already on screen must not jolt the view.
fn delta_to_reveal(what: Rect, view: Rect) -> Vec2 {
    let axis = |w0: f64, w1: f64, v0: f64, v1: f64| {
        if w0 < v0 {
            w0 - v0 // negative: scroll back
        } else if w1 > v1 {
            w1 - v1 // positive: scroll on
        } else {
            0.0
        }
    };
    Vec2::new(
        axis(what.x0, what.x1, view.x0, view.x1),
        axis(what.y0, what.y1, view.y0, view.y1),
    )
}

/// Resolve an AccessKit action request against the semantic tree.
///
/// Pure: no `Headless`, no window, no adapter — which is what makes it
/// testable. The shell's job is reduced to calling this and injecting the
/// result.
///
/// Returns `None` when the request cannot be honoured (unknown target,
/// unsupported action, missing or mistyped `ActionData`, or a scroll aimed at
/// something with nothing scrollable above it). `None` means "do nothing",
/// never "guess".
pub fn resolve_at_action(
    root: &SemanticsNode,
    target: u64,
    action: AkAction,
    data: Option<&ActionData>,
) -> Option<AtCommand> {
    let mut path: Vec<&SemanticsNode> = Vec::new();
    if !path_to(root, target, &mut path) {
        return None;
    }
    let node = *path.last()?;

    // The viewport to drive. For the scroll-by actions the target *is* the
    // scroller (an AT sends those to the scrollable node). For reveal-style
    // actions the target is the thing to show, so its own scroll extent is
    // irrelevant and the search starts at its parent.
    let scroller = |include_self: bool| -> Option<&SemanticsNode> {
        let end = if include_self {
            path.len()
        } else {
            path.len() - 1
        };
        path[..end].iter().rev().find(|n| n.scroll.is_some()).copied()
    };

    match action {
        AkAction::Click => Some(AtCommand::Click(center(node.bounds))),

        AkAction::ScrollUp | AkAction::ScrollDown | AkAction::ScrollLeft | AkAction::ScrollRight => {
            let sc = scroller(true)?;
            let step = match data {
                Some(ActionData::ScrollUnit(ScrollUnit::Page)) => {
                    (sc.bounds.height() - lumen_core::events::WHEEL_LINE_PX)
                        .max(lumen_core::events::WHEEL_LINE_PX)
                }
                // `Item` and an absent unit both mean "a line" — AccessKit
                // leaves the unit optional and the line is the safe default.
                _ => lumen_core::events::WHEEL_LINE_PX,
            };
            let delta = match action {
                AkAction::ScrollDown => Vec2::new(0.0, step),
                AkAction::ScrollUp => Vec2::new(0.0, -step),
                AkAction::ScrollRight => Vec2::new(step, 0.0),
                _ => Vec2::new(-step, 0.0),
            };
            Some(AtCommand::Wheel {
                pos: center(sc.bounds),
                delta,
            })
        }

        // The action that makes a virtualized list navigable: `set_size` tells
        // the AT there are 100 000 rows, and this is how it jumps to row
        // 50 000 — a node that does not exist yet and therefore cannot be
        // targeted directly.
        AkAction::SetScrollOffset => {
            let sc = scroller(true)?;
            let cur = sc.scroll.as_ref()?;
            let Some(ActionData::SetScrollOffset(p)) = data else {
                return None;
            };
            Some(AtCommand::Wheel {
                pos: center(sc.bounds),
                delta: Vec2::new(p.x - cur.x, p.y - cur.y),
            })
        }

        AkAction::ScrollIntoView => {
            let sc = scroller(false)?;
            Some(AtCommand::Wheel {
                pos: center(sc.bounds),
                delta: delta_to_reveal(node.bounds, sc.bounds),
            })
        }

        AkAction::ScrollToPoint => {
            let sc = scroller(false)?;
            let Some(ActionData::ScrollToPoint(p)) = data else {
                return None;
            };
            // Put the node's top-left at `p`: scroll by how far it currently
            // sits from there.
            Some(AtCommand::Wheel {
                pos: center(sc.bounds),
                delta: Vec2::new(node.bounds.x0 - p.x, node.bounds.y0 - p.y),
            })
        }

        _ => None,
    }
}

/// Map a Lumen [`Role`] to the closest AccessKit role.
pub fn role_to_accesskit(role: Role) -> AkRole {
    match role {
        Role::Window => AkRole::Window,
        Role::Button => AkRole::Button,
        Role::Checkbox => AkRole::CheckBox,
        Role::Radio => AkRole::RadioButton,
        Role::Switch => AkRole::Switch,
        Role::Slider => AkRole::Slider,
        Role::TextInput => AkRole::TextInput,
        Role::Text => AkRole::Label,
        Role::Image => AkRole::Image,
        Role::Link => AkRole::Link,
        Role::List => AkRole::List,
        Role::ListItem => AkRole::ListItem,
        Role::Table => AkRole::Table,
        Role::Row => AkRole::Row,
        Role::Cell => AkRole::Cell,
        Role::ColumnHeader => AkRole::ColumnHeader,
        Role::TabList => AkRole::TabList,
        Role::Tab => AkRole::Tab,
        Role::TabPanel => AkRole::TabPanel,
        Role::Menu => AkRole::Menu,
        Role::MenuItem => AkRole::MenuItem,
        Role::Dialog => AkRole::Dialog,
        Role::Alert => AkRole::Alert,
        Role::Tooltip => AkRole::Tooltip,
        Role::Progress => AkRole::ProgressIndicator,
        Role::Group => AkRole::Group,
        Role::ScrollArea => AkRole::ScrollView,
        Role::Tree => AkRole::Tree,
        Role::TreeItem => AkRole::TreeItem,
        Role::ComboBox => AkRole::ComboBox,
        Role::Generic => AkRole::GenericContainer,
    }
}

/// Apply Lumen [`State`]s onto an AccessKit node. Runtime-only states
/// (`Focused`/`Hovered`/`Pressed`) are not node properties — focus is carried
/// on the [`TreeUpdate`].
fn apply_states(node: &mut Node, states: &[State]) {
    for s in states {
        match s {
            State::Checked => node.set_toggled(Toggled::True),
            State::Unchecked => node.set_toggled(Toggled::False),
            State::Mixed => node.set_toggled(Toggled::Mixed),
            State::Selected => node.set_selected(true),
            State::Expanded => node.set_expanded(true),
            State::Collapsed => node.set_expanded(false),
            State::Disabled => node.set_disabled(),
            State::Readonly => node.set_read_only(),
            State::Required => node.set_required(),
            State::Busy => node.set_busy(),
            State::Invalid => node.set_label("invalid"), // surfaced via description elsewhere
            State::Focused | State::Hovered | State::Pressed => {}
        }
    }
}

/// Build an AccessKit [`TreeUpdate`] from a Lumen semantic tree (the elided
/// tree).
///
/// Node ids are `SemanticsNode.node.fold64()` — the 64-bit projection of the
/// node's structural [`NodeHandle`](lumen_core::identity::NodeHandle).
///
/// ID1 removed the previous `(path_salt << 32) | runtime_index` scheme. That
/// existed because raw-index ids let a reused index carry a stale parent
/// pointer into a pruned subtree, panicking `accesskit_consumer`'s diff
/// (`updated` node missing from the new state) — hit live on the wallet's
/// login→unlocked transition with AT-SPI active. The salt was a local fix for
/// the general problem `NodeHandle` now solves, so the whole
/// half-structural/half-positional construction is gone: the handle is already
/// the structural path, and unlike the old low half it does not change when the
/// arena recycles a slot.
pub fn build_tree(root: &SemanticsNode) -> TreeUpdate {
    let mut nodes = Vec::new();
    let mut focus_id = None;
    let root_id = build_node(root, &mut nodes, &mut focus_id);
    let focus = focus_id.unwrap_or(root_id);
    let mut tree = Tree::new(root_id);
    // P.4 set `tree.app_name` here, because without it the app showed as an
    // empty-name application in the AT-SPI registry. accesskit 0.24 removed the
    // field — not a regression to drop it: `accesskit_unix` now derives the
    // name itself from `std::env::current_exe()` (`context.rs::app_name`),
    // which is the same workaround, upstreamed. It keeps the file extension
    // where we took the stem; that is now the adapter's call to make.
    tree.toolkit_name = Some("Lumen".into());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
    // ID-0a's guard. Its value is NOT catching birthday collisions in the
    // 64-bit fold (~2.7e-12 at 10 000 nodes — it will never see one); it is
    // catching a derivation BUG that maps two distinct nodes to one id, which
    // would make AT clicks land on the wrong widget and would otherwise fail
    // only for screen-reader users, silently.
    debug_assert!(
        {
            let mut seen = std::collections::HashSet::with_capacity(nodes.len());
            nodes.iter().all(|(id, _)| seen.insert(*id))
        },
        "duplicate AccessKit node id in one TreeUpdate — two nodes are \
         indistinguishable to assistive tech"
    );

    TreeUpdate {
        nodes,
        tree: Some(tree),
        // accesskit 0.24 added subtrees: a `TreeUpdate` now says which tree it
        // applies to. Lumen publishes one tree per window through the adapter,
        // so every update is for the root tree. `Node::tree_id` grafts (the
        // subtree mechanism) are unused — if multi-window a11y ever wants a
        // real subtree per window, this is where it starts.
        tree_id: TreeId::ROOT,
        focus,
    }
}

fn build_node(
    n: &SemanticsNode,
    out: &mut Vec<(NodeId, Node)>,
    focus_out: &mut Option<NodeId>,
) -> NodeId {
    let id = NodeId(n.node.fold64());
    if n.states.contains(&State::Focused) {
        *focus_out = Some(id);
    }
    let mut node = Node::new(role_to_accesskit(n.role));
    if !n.label.is_empty() {
        node.set_label(n.label.clone());
        // The virtualization contract. A `VirtualList` puts a dozen rows in the
        // tree for a hundred thousand items; without these, a screen reader is
        // told the list HAS a dozen rows — a wrong answer, not a trade-off.
        if let Some(total) = n.set_size {
            node.set_size_of_set(total);
        }
        if let Some(pos) = n.position_in_set {
            node.set_position_in_set(pos);
        }
    }
    if let Some(v) = &n.value {
        node.set_value(v.clone());
    } else if n.role == Role::Text && !n.label.is_empty() {
        // P.4 (learned from the live AT-SPI smoke): static text must carry
        // its content as the *value* — AT-SPI exposes widget names but reads
        // label text from value/Text, so label-only text nodes were silent.
        node.set_value(n.label.clone());
    }
    // P.4: window-space bounds — ATs use these for spatial navigation,
    // magnifier tracking, and click-target resolution.
    node.set_bounds(accesskit::Rect {
        x0: n.bounds.x0,
        y0: n.bounds.y0,
        x1: n.bounds.x1,
        y1: n.bounds.y1,
    });
    apply_states(&mut node, &n.states);
    // P.4: declare supported actions — without them the platform exposes no
    // Action interface and ATs cannot activate the node (second live-smoke
    // finding). The default action maps to the same click path the pointer
    // and agent use.
    for a in &n.actions {
        match a {
            Action::Click => node.add_action(AkAction::Click),
            Action::Focus => node.add_action(AkAction::Focus),
            Action::Blur => node.add_action(AkAction::Blur),
            Action::SetValue => node.add_action(AkAction::SetValue),
            Action::Increment => node.add_action(AkAction::Increment),
            Action::Decrement => node.add_action(AkAction::Decrement),
            Action::ScrollIntoView => node.add_action(AkAction::ScrollIntoView),
            Action::Expand => node.add_action(AkAction::Expand),
            Action::Collapse => node.add_action(AkAction::Collapse),
            // No AccessKit dismiss action; Escape handles it everywhere.
            Action::Dismiss => {}
        }
    }
    // A11Y2c: the scroll actions are **derived** from the node reporting a
    // scroll extent, not declared by hand. A node with `ScrollInfo` is
    // scrollable by definition, so there is no state in which an author could
    // correctly omit these — and the two widgets that most needed keyboard
    // scrolling are exactly the ones that went without it for a release
    // because it had to be remembered per widget (A11Y2b).
    //
    // Only the axes that can actually move are declared: offering `ScrollLeft`
    // on a vertical list tells an AT a lie, and an AT that believes it will
    // report a control it cannot operate.
    if let Some(sc) = &n.scroll {
        if sc.max_y > 0.5 {
            node.add_action(AkAction::ScrollUp);
            node.add_action(AkAction::ScrollDown);
        }
        if sc.max_x > 0.5 {
            node.add_action(AkAction::ScrollLeft);
            node.add_action(AkAction::ScrollRight);
        }
        if sc.max_x > 0.5 || sc.max_y > 0.5 {
            // The one an AT needs to reach item 50 000 of a virtualized list:
            // `set_size` says how many there are, this is how it jumps there.
            node.add_action(AkAction::SetScrollOffset);
        }
    }
    let kids: Vec<NodeId> = n
        .children
        .iter()
        .map(|c| build_node(c, out, focus_out))
        .collect();
    node.set_children(kids);
    out.push((id, node));
    id
}
