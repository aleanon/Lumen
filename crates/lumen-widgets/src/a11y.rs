//! AccessKit integration (T4.3): maps Lumen's semantic [`Role`]/[`State`] onto
//! AccessKit's tree so the same one semantic tree drives platform a11y
//! (VoiceOver / NVDA / AT-SPI) — no separate accessibility pass.
//!
//! The role map is an exhaustive `match`, so adding a Lumen role fails to
//! compile until it is mapped here (the "map table complete" guarantee). See
//! `docs/a11y-checklist.md` for the manual VoiceOver/NVDA verification.

use accesskit::{Node, NodeId, Role as AkRole, Toggled, Tree, TreeUpdate};
use lumen_core::semantics::{Role, SemanticsNode, State};

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
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(root_id)),
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
    }
    apply_states(&mut node, &n.states);
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
