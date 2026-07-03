//! The semantic tree, its JSON export, and the selector engine (03 §1–§2).
//!
//! One tree drives accessibility, test locators, and agent observation (ADR-009)
//! — they can never drift because they are the same data. Pure layout nodes are
//! *elided* (their children splice into the parent); the selector grammar of
//! 03 §2 runs over the elided tree in document order.
//!
//! In M0 this module owns the schema, elision, JSON serialization, and the
//! selector resolver; the headless `App` (T0.9) builds these nodes during
//! rebuild. `allow(dead_code)` until then.

use crate::identity::StableId;
use kurbo::Rect;
#[cfg(feature = "snapshot")]
use serde_json::{json, Value};

/// Accessible role (closed set, 03 §1). Extend only via the decision log.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// The window root.
    Window,
    /// A push button.
    Button,
    /// A checkbox.
    Checkbox,
    /// A radio button.
    Radio,
    /// A switch.
    Switch,
    /// A slider.
    Slider,
    /// A text input.
    TextInput,
    /// Static text.
    Text,
    /// An image.
    Image,
    /// A hyperlink.
    Link,
    /// A list container.
    List,
    /// A list item.
    ListItem,
    /// A table.
    Table,
    /// A table row.
    Row,
    /// A table cell.
    Cell,
    /// A column header.
    ColumnHeader,
    /// A tab list.
    TabList,
    /// A tab.
    Tab,
    /// A tab panel.
    TabPanel,
    /// A menu.
    Menu,
    /// A menu item.
    MenuItem,
    /// A dialog.
    Dialog,
    /// An alert.
    Alert,
    /// A tooltip.
    Tooltip,
    /// A progress indicator.
    Progress,
    /// A generic grouping.
    Group,
    /// A scroll container.
    ScrollArea,
    /// A tree.
    Tree,
    /// A tree item.
    TreeItem,
    /// A combo box.
    ComboBox,
    /// Anything without a more specific role.
    Generic,
}

impl Role {
    /// The wire string for this role.
    pub fn as_str(self) -> &'static str {
        use Role::*;
        match self {
            Window => "window",
            Button => "button",
            Checkbox => "checkbox",
            Radio => "radio",
            Switch => "switch",
            Slider => "slider",
            TextInput => "text_input",
            Text => "text",
            Image => "image",
            Link => "link",
            List => "list",
            ListItem => "list_item",
            Table => "table",
            Row => "row",
            Cell => "cell",
            ColumnHeader => "column_header",
            TabList => "tab_list",
            Tab => "tab",
            TabPanel => "tab_panel",
            Menu => "menu",
            MenuItem => "menu_item",
            Dialog => "dialog",
            Alert => "alert",
            Tooltip => "tooltip",
            Progress => "progress",
            Group => "group",
            ScrollArea => "scroll_area",
            Tree => "tree",
            TreeItem => "tree_item",
            ComboBox => "combo_box",
            Generic => "generic",
        }
    }
}

/// Node state (03 §1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Has focus.
    Focused,
    /// Pointer is over it.
    Hovered,
    /// Currently pressed.
    Pressed,
    /// Disabled.
    Disabled,
    /// Checked.
    Checked,
    /// Unchecked.
    Unchecked,
    /// Mixed/indeterminate.
    Mixed,
    /// Selected.
    Selected,
    /// Expanded.
    Expanded,
    /// Collapsed.
    Collapsed,
    /// Read-only.
    Readonly,
    /// Required.
    Required,
    /// Invalid.
    Invalid,
    /// Busy.
    Busy,
}

impl State {
    /// The wire string for this state.
    pub fn as_str(self) -> &'static str {
        use State::*;
        match self {
            Focused => "focused",
            Hovered => "hovered",
            Pressed => "pressed",
            Disabled => "disabled",
            Checked => "checked",
            Unchecked => "unchecked",
            Mixed => "mixed",
            Selected => "selected",
            Expanded => "expanded",
            Collapsed => "collapsed",
            Readonly => "readonly",
            Required => "required",
            Invalid => "invalid",
            Busy => "busy",
        }
    }
}

/// An action a node supports (03 §1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Click/activate.
    Click,
    /// Focus.
    Focus,
    /// Blur.
    Blur,
    /// Set value.
    SetValue,
    /// Increment.
    Increment,
    /// Decrement.
    Decrement,
    /// Scroll into view.
    ScrollIntoView,
    /// Expand.
    Expand,
    /// Collapse.
    Collapse,
    /// Dismiss.
    Dismiss,
}

impl Action {
    /// The wire string for this action.
    pub fn as_str(self) -> &'static str {
        use Action::*;
        match self {
            Click => "click",
            Focus => "focus",
            Blur => "blur",
            SetValue => "set_value",
            Increment => "increment",
            Decrement => "decrement",
            ScrollIntoView => "scroll_into_view",
            Expand => "expand",
            Collapse => "collapse",
            Dismiss => "dismiss",
        }
    }
}

/// Scroll position for a scroll container.
#[derive(Clone, Copy, Debug)]
pub struct ScrollInfo {
    /// Horizontal offset.
    pub x: f64,
    /// Vertical offset.
    pub y: f64,
    /// Maximum horizontal offset.
    pub max_x: f64,
    /// Maximum vertical offset.
    pub max_y: f64,
}

/// A text selection range (byte offsets).
#[derive(Clone, Copy, Debug)]
pub struct TextSelection {
    /// Selection start.
    pub start: usize,
    /// Selection end.
    pub end: usize,
}

/// Typographic metrics for a text node (diagnostic aid alongside the
/// authoritative [`ink`](SemanticsNode::ink) clip check). `content_height` is the
/// font's *declared* extent (sum of per-line ascent+descent); exceeding
/// `box_height` hints the line-height is tighter than the font wants, which names
/// the line-height class behind a W0104 warning.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMetrics {
    /// Number of (wrapped) lines.
    pub line_count: u32,
    /// Reserved block height (logical px).
    pub box_height: f32,
    /// Max typographic ascent across lines.
    pub ascent: f32,
    /// Max typographic descent across lines.
    pub descent: f32,
    /// Max per-line box height across lines.
    pub line_height: f32,
    /// Sum of each line's ascent+descent — the glyph extent.
    pub content_height: f32,
}

/// A semantic node (03 §1). Built during rebuild; `elide` marks pure-layout
/// nodes whose children splice into the parent.
#[derive(Clone, Debug)]
pub struct SemanticsNode {
    /// Runtime node index (serializes as `"node-<index>"`).
    pub node: u32,
    /// Author id, if set.
    pub id: Option<StableId>,
    /// Role.
    pub role: Role,
    /// Accessible name.
    pub label: String,
    /// Current value (inputs/sliders).
    pub value: Option<String>,
    /// CSS-style classes.
    pub classes: Vec<String>,
    /// Active states.
    pub states: Vec<State>,
    /// Window-space bounds (the layout box).
    pub bounds: Rect,
    /// Rendered *ink* bounds — what the node actually painted. For text this can
    /// extend past `bounds` (descenders/side bearings); when it does, content is
    /// being clipped. `None` ⇒ ink coincides with `bounds`.
    pub ink: Option<Rect>,
    /// Typographic metrics for text nodes (diagnostic aid; `None` for non-text).
    pub text_metrics: Option<TextMetrics>,
    /// Supported actions.
    pub actions: Vec<Action>,
    /// Scroll info (scroll containers only).
    pub scroll: Option<ScrollInfo>,
    /// Text selection (text inputs only).
    pub text_selection: Option<TextSelection>,
    /// Rust widget type name (debug aid).
    pub type_name: String,
    /// If this node is the root of a `cx.scope`, the stable keys of the signals
    /// that scope depends on — the reactive graph projected into observability
    /// (F2). Lets the agent see *why* a subtree updates. `None` ⇒ not a scope
    /// root (or a scope that read no state).
    pub deps: Option<Vec<String>>,
    /// Whether this node is elided (pure layout, no semantic contribution).
    pub elide: bool,
    /// Children (raw, pre-elision).
    pub children: Vec<SemanticsNode>,
}

impl SemanticsNode {
    /// A minimal node with the given role.
    pub fn new(node: u32, role: Role) -> SemanticsNode {
        SemanticsNode {
            node,
            id: None,
            role,
            label: String::new(),
            value: None,
            classes: Vec::new(),
            states: Vec::new(),
            bounds: Rect::ZERO,
            ink: None,
            text_metrics: None,
            actions: Vec::new(),
            scroll: None,
            text_selection: None,
            type_name: String::new(),
            deps: None,
            elide: false,
            children: Vec::new(),
        }
    }

    /// Return this node's elided children: any child marked `elide` is replaced
    /// by its own (recursively elided) children.
    fn elided_children(&self) -> Vec<SemanticsNode> {
        let mut out = Vec::new();
        for child in &self.children {
            if child.elide {
                out.extend(child.elided_children());
            } else {
                let mut c = child.clone();
                c.children = child.elided_children();
                out.push(c);
            }
        }
        out
    }

    /// A copy of this subtree with elision applied (root is never elided).
    pub fn elided(&self) -> SemanticsNode {
        let mut c = self.clone();
        c.children = self.elided_children();
        c
    }

    #[cfg(feature = "snapshot")]
    fn to_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("node".into(), json!(format!("node-{}", self.node)));
        if let Some(id) = &self.id {
            obj.insert("id".into(), json!(id.as_str()));
        }
        obj.insert("role".into(), json!(self.role.as_str()));
        obj.insert("label".into(), json!(self.label));
        if let Some(v) = &self.value {
            obj.insert("value".into(), json!(v));
        }
        obj.insert("classes".into(), json!(self.classes));
        obj.insert(
            "states".into(),
            Value::Array(self.states.iter().map(|s| json!(s.as_str())).collect()),
        );
        obj.insert(
            "bounds".into(),
            json!({"x": self.bounds.x0, "y": self.bounds.y0, "w": self.bounds.width(), "h": self.bounds.height()}),
        );
        obj.insert(
            "actions".into(),
            Value::Array(self.actions.iter().map(|a| json!(a.as_str())).collect()),
        );
        if let Some(s) = &self.scroll {
            obj.insert(
                "scroll".into(),
                json!({"x": s.x, "y": s.y, "max_x": s.max_x, "max_y": s.max_y}),
            );
        }
        if let Some(ts) = &self.text_selection {
            obj.insert(
                "text_selection".into(),
                json!({"start": ts.start, "end": ts.end}),
            );
        }
        obj.insert("type".into(), json!(self.type_name));
        if let Some(deps) = &self.deps {
            obj.insert("deps".into(), json!(deps));
        }
        obj.insert(
            "children".into(),
            Value::Array(self.children.iter().map(|c| c.to_json()).collect()),
        );
        Value::Object(obj)
    }
}

/// Window-level info for a semantics document.
#[derive(Clone, Copy, Debug)]
pub struct WindowInfo {
    /// Window width (logical px).
    pub width: f64,
    /// Window height (logical px).
    pub height: f64,
    /// DPI scale factor.
    pub scale: f64,
    /// Focused node index, if any.
    pub focused: Option<u32>,
}

/// A complete semantics document (the value of `Headless::semantics_json` /
/// `ui.getTree`, 03 §1).
#[derive(Clone, Debug)]
pub struct SemanticsDoc {
    /// Window info.
    pub window: WindowInfo,
    /// The (already elided unless `raw`) root node.
    pub root: SemanticsNode,
}

impl SemanticsDoc {
    /// Serialize to the `lumen-semantics/1` JSON shape. `raw` returns the
    /// unelided tree (`ui.getTree {raw:true}`). Available only in a `snapshot`
    /// build (the agent's introspection path); lean builds omit it.
    #[cfg(feature = "snapshot")]
    pub fn to_json(&self, raw: bool) -> Value {
        let root = if raw {
            self.root.clone()
        } else {
            self.root.elided()
        };
        let mut window = serde_json::Map::new();
        window.insert("width".into(), json!(self.window.width));
        window.insert("height".into(), json!(self.window.height));
        window.insert("scale".into(), json!(self.window.scale));
        if let Some(f) = self.window.focused {
            window.insert("focused".into(), json!(format!("node-{f}")));
        }
        json!({
            "schema": "lumen-semantics/1",
            "window": Value::Object(window),
            "root": root.to_json(),
        })
    }
}

// ---------------------------------------------------------------------------
// Selector engine (03 §2)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Part {
    Id(String),
    Class(String),
    Role(String),
    State(String),
    TextEq(String),
    TextContains(String),
    Has(Box<Selector>),
    Any,
}

#[derive(Clone, Debug, PartialEq)]
struct Compound {
    parts: Vec<Part>,
    nth: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Combinator {
    Descendant,
    Child,
}

/// A parsed selector (03 §2).
#[derive(Clone, Debug, PartialEq)]
pub struct Selector {
    first: Compound,
    rest: Vec<(Combinator, Compound)>,
}

/// A selector parse error.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectorParseError(pub String);

impl std::fmt::Display for SelectorParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid selector: {}", self.0)
    }
}

/// Why resolving a selector to a single node failed.
#[derive(Clone, Debug, PartialEq)]
pub enum ResolveError {
    /// The selector did not parse.
    Parse(String),
    /// No node matched. `nearest` lists near-miss node indices.
    NotFound {
        /// Near-miss candidate node indices.
        nearest: Vec<u32>,
    },
    /// More than one node matched. `candidates` lists their node indices.
    Ambiguous {
        /// Matching node indices, in document order.
        candidates: Vec<u32>,
    },
}

impl Selector {
    /// Parse a selector string.
    pub fn parse(s: &str) -> Result<Selector, SelectorParseError> {
        parse_selector(s)
    }
}

// Flat view of the elided tree for matching.
struct Flat {
    nodes: Vec<FlatNode>,
}

struct FlatNode {
    node: u32,
    id: Option<String>,
    role: &'static str,
    label: String,
    classes: Vec<String>,
    states: Vec<&'static str>,
    parent: Option<usize>,
    children: Vec<usize>,
}

impl Flat {
    fn build(root: &SemanticsNode) -> Flat {
        let mut nodes = Vec::new();
        build_flat(root, None, &mut nodes);
        Flat { nodes }
    }
}

fn build_flat(n: &SemanticsNode, parent: Option<usize>, out: &mut Vec<FlatNode>) -> usize {
    let idx = out.len();
    out.push(FlatNode {
        node: n.node,
        id: n.id.as_ref().map(|i| i.as_str().to_string()),
        role: n.role.as_str(),
        label: n.label.clone(),
        classes: n.classes.clone(),
        states: n.states.iter().map(|s| s.as_str()).collect(),
        parent,
        children: Vec::new(),
    });
    let mut child_idx = Vec::new();
    for c in &n.children {
        child_idx.push(build_flat(c, Some(idx), out));
    }
    out[idx].children = child_idx;
    idx
}

/// Resolve a selector over `root` (which is treated as already elided) to all
/// matching node indices, in document order.
pub fn select(root: &SemanticsNode, selector: &str) -> Result<Vec<u32>, SelectorParseError> {
    let sel = Selector::parse(selector)?;
    let flat = Flat::build(root);
    let matched = match_selector(&flat, &sel);
    Ok(matched.into_iter().map(|i| flat.nodes[i].node).collect())
}

/// Resolve a selector to exactly one node (the locator/agent contract, 03 §2),
/// returning structured `NotFound`/`Ambiguous` errors otherwise.
pub fn resolve_one(root: &SemanticsNode, selector: &str) -> Result<u32, ResolveError> {
    let sel = Selector::parse(selector).map_err(|e| ResolveError::Parse(e.0))?;
    let flat = Flat::build(root);
    let matched = match_selector(&flat, &sel);
    match matched.len() {
        1 => Ok(flat.nodes[matched[0]].node),
        0 => Err(ResolveError::NotFound {
            nearest: nearest_miss(&flat, &sel),
        }),
        _ => Err(ResolveError::Ambiguous {
            candidates: matched.into_iter().map(|i| flat.nodes[i].node).collect(),
        }),
    }
}

fn match_selector(flat: &Flat, sel: &Selector) -> Vec<usize> {
    // initial set: nodes matching the first compound, in document order
    let mut cur: Vec<usize> = (0..flat.nodes.len())
        .filter(|&i| node_matches(flat, i, &sel.first.parts))
        .collect();
    apply_nth(&mut cur, sel.first.nth);

    for (comb, comp) in &sel.rest {
        let candidates: Vec<usize> = (0..flat.nodes.len())
            .filter(|&i| node_matches(flat, i, &comp.parts))
            .collect();
        let mut next = Vec::new();
        for i in candidates {
            let ok = match comb {
                Combinator::Child => flat.nodes[i]
                    .parent
                    .map(|p| cur.contains(&p))
                    .unwrap_or(false),
                Combinator::Descendant => ancestors(flat, i).any(|a| cur.contains(&a)),
            };
            if ok {
                next.push(i);
            }
        }
        apply_nth(&mut next, comp.nth);
        cur = next;
    }
    cur
}

fn apply_nth(set: &mut Vec<usize>, nth: Option<usize>) {
    if let Some(n) = nth {
        // 1-based
        let picked = if n >= 1 && n <= set.len() {
            vec![set[n - 1]]
        } else {
            Vec::new()
        };
        *set = picked;
    }
}

fn ancestors(flat: &Flat, i: usize) -> impl Iterator<Item = usize> + '_ {
    let mut cur = flat.nodes[i].parent;
    std::iter::from_fn(move || {
        let n = cur?;
        cur = flat.nodes[n].parent;
        Some(n)
    })
}

fn node_matches(flat: &Flat, i: usize, parts: &[Part]) -> bool {
    let n = &flat.nodes[i];
    parts.iter().all(|p| match p {
        Part::Any => true,
        Part::Id(s) => n.id.as_deref() == Some(s.as_str()),
        Part::Class(s) => n.classes.iter().any(|c| c == s),
        Part::Role(s) => n.role == s.as_str(),
        Part::State(s) => n.states.contains(&s.as_str()),
        Part::TextEq(s) => n.label.trim() == s,
        Part::TextContains(s) => n.label.to_lowercase().contains(&s.to_lowercase()),
        Part::Has(inner) => {
            // any descendant matches the inner selector
            let sub = match_selector(flat, inner);
            sub.iter().any(|&m| ancestors(flat, m).any(|a| a == i))
        }
    })
}

fn nearest_miss(flat: &Flat, sel: &Selector) -> Vec<u32> {
    // Heuristic: nodes matching the final compound's parts, ignoring structure.
    let last = sel.rest.last().map(|(_, c)| c).unwrap_or(&sel.first);
    (0..flat.nodes.len())
        .filter(|&i| node_matches(flat, i, &last.parts))
        .map(|i| flat.nodes[i].node)
        .collect()
}

// ----- selector parsing -----------------------------------------------------

fn parse_selector(input: &str) -> Result<Selector, SelectorParseError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(SelectorParseError("empty selector".into()));
    }
    let mut compounds: Vec<(Option<Combinator>, Compound)> = Vec::new();
    let mut chars = s.chars().peekable();
    let mut pending_comb: Option<Combinator> = None;
    let mut saw_ws = false;

    loop {
        // skip whitespace, remembering it implies a descendant combinator
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                saw_ws = true;
                chars.next();
            } else {
                break;
            }
        }
        let Some(&c) = chars.peek() else { break };
        if c == '>' {
            chars.next();
            pending_comb = Some(Combinator::Child);
            saw_ws = false;
            continue;
        }
        if saw_ws && pending_comb.is_none() && !compounds.is_empty() {
            pending_comb = Some(Combinator::Descendant);
        }
        saw_ws = false;
        let compound = parse_compound(&mut chars)?;
        let comb = if compounds.is_empty() {
            None
        } else {
            Some(pending_comb.unwrap_or(Combinator::Descendant))
        };
        compounds.push((comb, compound));
        pending_comb = None;
    }

    if compounds.is_empty() {
        return Err(SelectorParseError("empty selector".into()));
    }
    let mut it = compounds.into_iter();
    let first = it.next().unwrap().1;
    let mut rest = Vec::new();
    for (comb, comp) in it {
        rest.push((comb.unwrap_or(Combinator::Descendant), comp));
    }
    Ok(Selector { first, rest })
}

fn parse_compound(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<Compound, SelectorParseError> {
    let mut parts = Vec::new();
    let mut nth = None;
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() || c == '>' => break,
            '*' => {
                chars.next();
                parts.push(Part::Any);
            }
            '#' => {
                chars.next();
                parts.push(Part::Id(parse_ident(chars)?));
            }
            '.' => {
                chars.next();
                parts.push(Part::Class(parse_ident(chars)?));
            }
            ':' => {
                chars.next();
                let name = parse_ident(chars)?;
                match name.as_str() {
                    "text" => parts.push(Part::TextEq(parse_paren_string(chars)?)),
                    "text-contains" => parts.push(Part::TextContains(parse_paren_string(chars)?)),
                    "has" => parts.push(Part::Has(Box::new(parse_paren_selector(chars)?))),
                    "nth" => nth = Some(parse_paren_int(chars)?),
                    other => parts.push(Part::State(other.to_string())),
                }
            }
            c if is_ident_char(c) => {
                parts.push(Part::Role(parse_ident(chars)?));
            }
            other => {
                return Err(SelectorParseError(format!("unexpected '{other}'")));
            }
        }
    }
    if parts.is_empty() && nth.is_none() {
        return Err(SelectorParseError("empty compound".into()));
    }
    Ok(Compound { parts, nth })
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

fn parse_ident(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<String, SelectorParseError> {
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if is_ident_char(c) {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if s.is_empty() {
        return Err(SelectorParseError("expected identifier".into()));
    }
    Ok(s)
}

fn parse_paren_string(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<String, SelectorParseError> {
    expect(chars, '(')?;
    expect(chars, '"')?;
    let mut s = String::new();
    let mut closed = false;
    for c in chars.by_ref() {
        if c == '"' {
            closed = true;
            break;
        }
        s.push(c);
    }
    if !closed {
        return Err(SelectorParseError("unterminated string".into()));
    }
    expect(chars, ')')?;
    Ok(s)
}

fn parse_paren_int(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<usize, SelectorParseError> {
    expect(chars, '(')?;
    let mut s = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            s.push(c);
            chars.next();
        } else {
            break;
        }
    }
    expect(chars, ')')?;
    s.parse::<usize>()
        .map_err(|_| SelectorParseError("expected integer".into()))
}

fn parse_paren_selector(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> Result<Selector, SelectorParseError> {
    expect(chars, '(')?;
    let mut depth = 1;
    let mut inner = String::new();
    for c in chars.by_ref() {
        match c {
            '(' => {
                depth += 1;
                inner.push(c);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return parse_selector(&inner);
                }
                inner.push(c);
            }
            _ => inner.push(c),
        }
    }
    Err(SelectorParseError("unterminated :has()".into()))
}

fn expect(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    expected: char,
) -> Result<(), SelectorParseError> {
    match chars.next() {
        Some(c) if c == expected => Ok(()),
        Some(c) => Err(SelectorParseError(format!(
            "expected '{expected}', got '{c}'"
        ))),
        None => Err(SelectorParseError(format!("expected '{expected}'"))),
    }
}

#[cfg(test)]
mod tests;
