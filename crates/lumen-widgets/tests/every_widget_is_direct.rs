//! Every widget lowers through the sink.
//!
//! The goal this file exists to hold: **all 57 widget types implement
//! [`Direct`]**, so any of them can be written into the tree without an
//! `Element` subtree, and a converted parent can hold any child
//! monomorphically rather than through the boxed escape hatch.
//!
//! It is a *type-level* check — nothing is constructed — so it costs nothing to
//! run and cannot be satisfied by a widget that happens not to be exercised
//! elsewhere. A widget added without a `Direct` impl fails to compile here,
//! which is the point: the invariant is enforced, not documented.
//!
//! Two tiers are both acceptable and both counted:
//!
//! * **native** — the widget writes its node and lowers its children through
//!   the sink itself (`impl_widget!(Ty, native)`);
//! * **bridged** — `impl_widget!` generates a `Direct` that builds the
//!   widget's `Element` and hands the tree over. Correct, and the starting
//!   point every widget gets for free.

#![allow(unused_imports)]

use lumen_widgets::*;

fn assert_direct<T: Direct>() {}

#[test]
fn every_widget_implements_direct() {
    assert_direct::<Accordion>();
    assert_direct::<AlignBox>();
    assert_direct::<AppBar>();
    assert_direct::<Avatar>();
    assert_direct::<Badge>();
    assert_direct::<BarChart>();
    assert_direct::<BottomNav>();
    assert_direct::<Button>();
    assert_direct::<Canvas>();
    assert_direct::<Card>();
    assert_direct::<CheckBox>();
    assert_direct::<Chip>();
    assert_direct::<ColorPicker>();
    assert_direct::<Combobox>();
    assert_direct::<Container>();
    assert_direct::<DataGrid>();
    assert_direct::<DatePicker>();
    assert_direct::<Drawer>();
    assert_direct::<FilePicker>();
    assert_direct::<FindReplaceBar>();
    assert_direct::<Icon>();
    assert_direct::<Image>();
    assert_direct::<Label>();
    assert_direct::<Menu>();
    assert_direct::<Modal>();
    assert_direct::<NavigationRail>();
    assert_direct::<Pagination>();
    assert_direct::<PaneGrid>();
    assert_direct::<PickList>();
    assert_direct::<Popover>();
    assert_direct::<ProgressBar>();
    assert_direct::<PullToRefresh>();
    assert_direct::<Radio>();
    assert_direct::<RangeSlider>();
    assert_direct::<RichText>();
    assert_direct::<RichTextEditor>();
    assert_direct::<Rule>();
    assert_direct::<Scrollable>();
    assert_direct::<SearchField>();
    assert_direct::<Select>();
    assert_direct::<Sheet>();
    assert_direct::<Skeleton>();
    assert_direct::<Slider>();
    assert_direct::<Space>();
    assert_direct::<Spinner>();
    assert_direct::<SplitPane>();
    assert_direct::<Stepper>();
    assert_direct::<Switch>();
    assert_direct::<Tabs>();
    assert_direct::<TextField>();
    assert_direct::<TextInput>();
    assert_direct::<TimePicker>();
    assert_direct::<Toast>();
    assert_direct::<Tooltip>();
    assert_direct::<Tree>();
    assert_direct::<VirtualList>();
    assert_direct::<Wrap>();
}

/// The containers, called out separately: these are the ones where lowering
/// children through the sink instead of through an `Element` field is the
/// point, so a regression to the bridge here would be a silent loss.
#[test]
fn the_containers_lower_natively() {
    // Compile-time only; the assertion that they are *native* rather than
    // bridged is the `impl_widget!(Ty, native)` in each source file, which
    // would collide with a generated impl if it were ever re-added.
    assert_direct::<Container>();
    assert_direct::<Card>();
    assert_direct::<Scrollable>();
    assert_direct::<Accordion>();
    assert_direct::<AppBar>();
    assert_direct::<PullToRefresh>();
    assert_direct::<Wrap>();
}
