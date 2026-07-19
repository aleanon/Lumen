//! AccessKit integration (T4.3): maps Lumen's semantic [`Role`]/[`State`] onto
//! AccessKit's tree so the same one semantic tree drives platform a11y
//! (VoiceOver / NVDA / AT-SPI) — no separate accessibility pass.
//!
//! The role map is an exhaustive `match`, so adding a Lumen role fails to
//! compile until it is mapped here (the "map table complete" guarantee). See
//! `docs/a11y-checklist.md` for the manual VoiceOver/NVDA verification.

use accesskit::{Action as AkAction, Node, NodeId, Role as AkRole, Toggled, Tree, TreeUpdate};
use lumen_core::semantics::{Action, Role, SemanticsNode, State};

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
/// tree). Node ids reuse the runtime node index.
pub fn build_tree(root: &SemanticsNode) -> TreeUpdate {
    let mut nodes = Vec::new();
    let root_id = build_node(root, &mut nodes);
    let focus = find_focus(root).unwrap_or(root_id);
    let mut tree = Tree::new(root_id);
    // P.4: identify the app on the a11y bus (it showed as an empty-name
    // application in the AT-SPI registry without this). The binary name is
    // the best per-app identity the framework has; the toolkit fields are
    // fixed.
    tree.app_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
    tree.toolkit_name = Some("Lumen".into());
    tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").into());
    TreeUpdate {
        nodes,
        tree: Some(tree),
        focus,
    }
}

fn build_node(n: &SemanticsNode, out: &mut Vec<(NodeId, Node)>) -> NodeId {
    let id = NodeId(n.node as u64);
    let mut node = Node::new(role_to_accesskit(n.role));
    if !n.label.is_empty() {
        node.set_label(n.label.clone());
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
    let kids: Vec<NodeId> = n.children.iter().map(|c| build_node(c, out)).collect();
    node.set_children(kids);
    out.push((id, node));
    id
}

fn find_focus(n: &SemanticsNode) -> Option<NodeId> {
    if n.states.iter().any(|s| matches!(s, State::Focused)) {
        return Some(NodeId(n.node as u64));
    }
    n.children.iter().find_map(find_focus)
}
