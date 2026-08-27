//! Step 3 — the side table, columnar.
//!
//! `Meta` is 656 bytes of uniform record per node in a `HashMap` — the same
//! shape of problem `Element` was, one layer down. `MetaStore` splits it into
//! columns indexed densely by the node's arena index.
//!
//! Speed is the easy part; the risk is correctness. A dense array indexed by
//! arena slot is only safe if a **stale** `NodeIndex` — one whose slot has been
//! freed and handed out again — reads as absent rather than as whatever now
//! occupies that slot. Getting that wrong would hand the agent one node's
//! semantics under another node's identity, silently.

use lumen_core::semantics::Role;
use lumen_core::tree::Tree;
use lumen_core::Color;
use lumen_widgets::direct::{MetaFlags, MetaStore, NodeId, Sym};

#[test]
fn columns_round_trip_every_hot_field() {
    let mut t = Tree::new();
    let r = t.insert_orphan();
    t.set_root(r);
    let a = t.insert_child(r);
    let b = t.insert_child(r);

    let mut s = MetaStore::default();
    s.insert(a, Role::Button);
    s.set_background(a, Some(Color::srgb8(0x11, 0x22, 0x33, 0xff)));
    s.set_corner_radius(a, 6.0);
    s.set_flags(a, MetaFlags::FOCUSABLE, true);
    s.set_node_id(a, NodeId::at(Sym(3), 7));
    s.push_class(a, Sym(9));

    s.insert(b, Role::Text);

    assert_eq!(s.role(a), Role::Button);
    assert_eq!(s.role(b), Role::Text, "columns do not bleed between slots");
    assert_eq!(s.corner_radius(a), 6.0);
    assert_eq!(s.corner_radius(b), 0.0);
    assert!(s.flags(a).contains(MetaFlags::FOCUSABLE));
    assert!(!s.flags(b).contains(MetaFlags::FOCUSABLE));
    assert_eq!(s.node_id(a), Some(NodeId::at(Sym(3), 7)));
    assert_eq!(s.node_id(b), None);
    assert_eq!(s.class_syms(a).len(), 1);
    assert_eq!(s.class_syms(b).len(), 0);
}

#[test]
fn a_stale_node_index_reads_as_absent() {
    // The whole safety argument for dense columns. Free a node, let the arena
    // hand its slot to a new one, and check the old handle does not read the
    // new node's record.
    let mut t = Tree::new();
    let r = t.insert_orphan();
    t.set_root(r);
    let old = t.insert_child(r);

    let mut s = MetaStore::default();
    s.insert(old, Role::Button);
    assert!(s.contains(old));

    t.detach(old);
    t.free_one(old);
    s.remove(old);

    let new = t.insert_child(r);
    s.insert(new, Role::Dialog);

    assert!(s.contains(new), "the new node is live");
    assert_eq!(s.role(new), Role::Dialog);
    assert!(
        !s.contains(old),
        "the stale handle reads absent — if this passed by index alone it \
         would return the Dialog's record under the Button's identity"
    );
}

#[test]
fn a_freed_node_stops_being_live() {
    let mut t = Tree::new();
    let r = t.insert_orphan();
    t.set_root(r);
    let n = t.insert_child(r);
    let mut s = MetaStore::default();
    s.insert(n, Role::Button);
    assert!(s.contains(n));
    s.remove(n);
    assert!(!s.contains(n), "removed records are not live");
}

#[test]
fn the_cold_half_is_allocated_only_when_used() {
    // The uniformity half of the argument: a node that is not a text field must
    // not pay for `caret_byte`, twelve handler slots and a label `String`.
    let mut t = Tree::new();
    let r = t.insert_orphan();
    t.set_root(r);
    let plain = t.insert_child(r);
    let rich = t.insert_child(r);

    let mut s = MetaStore::default();
    s.insert(plain, Role::Generic);
    s.insert(rich, Role::TextInput);
    assert_eq!(s.cold_count(), 0, "nothing rare has been touched yet");

    s.cold_mut(rich).label = "Name".to_string();
    assert_eq!(s.cold_count(), 1, "only the node that needed it pays");
    assert_eq!(s.cold(rich).map(|c| c.label.as_str()), Some("Name"));
    assert!(
        s.cold(plain).is_none(),
        "and the plain node still pays nothing"
    );
}

#[test]
fn reinserting_a_slot_clears_the_previous_record() {
    // Slots are reused. A fresh insert must not inherit the last tenant's
    // background, classes or flags.
    let mut t = Tree::new();
    let r = t.insert_orphan();
    t.set_root(r);
    let n = t.insert_child(r);

    let mut s = MetaStore::default();
    s.insert(n, Role::Button);
    s.set_background(n, Some(Color::WHITE));
    s.set_flags(n, MetaFlags::DISABLED, true);
    s.push_class(n, Sym(1));
    s.cold_mut(n).label = "old".to_string();

    s.insert(n, Role::Text);
    assert_eq!(s.role(n), Role::Text);
    assert_eq!(s.background(n), None, "the fill did not survive");
    assert_eq!(s.flags(n), MetaFlags::empty(), "nor the flags");
    assert_eq!(s.class_syms(n).len(), 0, "nor the classes");
    assert!(s.cold(n).is_none(), "nor the cold record");
}
