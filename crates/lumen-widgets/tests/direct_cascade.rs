//! WT-EXP — can the `.lss` cascade *compose* instead of mutating an element?
//!
//! `build_node` runs the cascade by writing into the element sitting between
//! the widget and taffy (`apply_css_to_element(&mut el, &css)`). With no
//! element there is nothing to write into, so the direct path resolves and then
//! folds the result onto the style it hands taffy and the paint it hands the
//! side table.
//!
//! These tests drive real stylesheets through the sink and check the four
//! things that could plausibly break: a plain rule, a descendant selector (does
//! the ancestor chain survive a builder?), an inherited `:disabled` state, and
//! specificity between a class and an id.

use lumen_core::semantics::Role;
use lumen_style::{MediaContext, StyleSource, Tokens};
use lumen_widgets::direct::{begin_row, row_style, Direct, StyleEnv, TreeSink, VisualState};
use lumen_widgets::{Button, Label};

fn env(src: &str) -> StyleEnv {
    let (sheet, diags) = lumen_style::parse("test.lss", src);
    assert!(diags.is_empty(), "stylesheet has diagnostics: {diags:?}");
    StyleEnv {
        sources: vec![StyleSource {
            sheet,
            origin: lumen_style::Origin::App,
        }],
        tokens: Tokens::default(),
        media: MediaContext::default(),
    }
}

fn sink(src: &str) -> TreeSink {
    TreeSink::new().with_styles(env(src), VisualState::default())
}

#[test]
fn a_rule_reaches_the_layout_style_through_composition() {
    // `.wide` sets a width the widget never asked for.
    let mut s = sink(".wide { width: 320px; }");
    let n = s.begin(None, Role::Group);
    s.class(n, "wide".to_string());
    s.resolve(n);
    let ln = s.end(n, &Default::default(), &[], false);

    // The width reached taffy, so it came through `end`'s fold rather than a
    // mutation of some element in between.
    let _ = ln;
    assert_eq!(s.meta[&n].layout_style.width, lumen_layout::Dim::px(320.0));
}

#[test]
fn a_rule_reaches_the_paint_side_table() {
    let mut s = sink("#save { background: #ff0000; border-radius: 3px; }");
    let n = s.begin(None, Role::Button);
    s.id(n, "save".into());
    s.resolve(n);
    s.end(n, &Default::default(), &[], false);

    let m = &s.meta[&n];
    let bg = m.background.expect("background from the sheet");
    assert!(bg.r > 0.9 && bg.g < 0.1, "red reached the node: {bg:?}");
    assert_eq!(m.corner_radius, 3.0);
}

#[test]
fn descendant_selectors_survive_the_builder() {
    // The ancestor chain is what `resolve` pushes and `end` pops. If a builder
    // could not maintain it, this is the test that would fail: the same button
    // matches or not purely on where it sits.
    let mut s = sink("dialog button { background: #00ff00; }");

    let outside = s.begin(None, Role::Button);
    s.resolve(outside);
    s.end(outside, &Default::default(), &[], false);

    let dialog = s.begin(None, Role::Dialog);
    s.resolve(dialog);
    let inside = s.begin(Some(dialog), Role::Button);
    s.resolve(inside);
    let ln = s.end(inside, &Default::default(), &[], false);
    s.end(dialog, &Default::default(), &[ln], false);

    assert!(
        s.meta[&inside].background.is_some(),
        "a button inside a dialog matches `dialog button`"
    );
    assert!(
        s.meta[&outside].background.is_none(),
        "a button outside it does not — the ancestor chain is real, not a \
         rightmost-compound match"
    );
}

#[test]
fn disabled_is_inherited_by_descendants() {
    let mut s = sink("button:disabled { background: #808080; }");
    let group = s.begin(None, Role::Group);
    s.disabled(group, true);
    s.resolve(group);
    // The engine enters the disabled subtree, as `build_node` does with its
    // `disabled_count`.
    s.enter_disabled();
    let child = s.begin(Some(group), Role::Button);
    s.resolve(child);
    let ln = s.end(child, &Default::default(), &[], false);
    s.exit_disabled();
    s.end(group, &Default::default(), &[ln], true);

    assert!(
        s.meta[&child].background.is_some(),
        "a button inside a disabled group matches `:disabled` even though the \
         button itself was never marked disabled"
    );
}

#[test]
fn an_id_rule_beats_a_class_rule() {
    let mut s = sink(".b { width: 100px; } #special { width: 200px; }");
    let n = s.begin(None, Role::Group);
    s.class(n, "b".to_string());
    s.id(n, "special".into());
    s.resolve(n);
    let _ln = s.end(n, &Default::default(), &[], false);
    assert_eq!(
        s.meta[&n].layout_style.width,
        lumen_layout::Dim::px(200.0),
        "specificity is the cascade's job and survives composition unchanged"
    );
}

#[test]
fn the_sheet_overrides_what_the_widget_asked_for() {
    // A real widget with its own opinion about its style, overruled by .lss.
    let mut s = sink("button { width: 500px; background: #0000ff; }");
    let (n, ln) = {
        let n = s.begin(None, Role::Button);
        s.label(n, "Save".to_string());
        s.resolve(n);
        let ln = s.end(n, &lumen_layout::LayoutStyle::default(), &[], false);
        (n, ln)
    };
    let _ = ln;
    assert_eq!(s.meta[&n].layout_style.width, lumen_layout::Dim::px(500.0));
    let bg = s.meta[&n].background.expect("sheet background");
    assert!(bg.b > 0.9, "sheet fill wins over the widget's own: {bg:?}");
}

#[test]
fn a_real_widget_lowers_through_the_cascade() {
    let mut s = sink("button { border-radius: 2px; } text { color: #123456; }");
    let root = begin_row(&mut s, None);
    s.resolve(root);
    let (lab, a) = Label::new("hello").size(14.0).lower(&mut s, Some(root));
    let (btn, b) = Button::new("Go").lower(&mut s, Some(root));
    s.end(root, &row_style(8.0, 0.0), &[a, b], false);

    assert_eq!(
        s.meta[&btn].corner_radius, 2.0,
        "the sheet overrode Button's own 8px radius"
    );
    let _ = lab;
}

/// The hazard the direct design introduces, made concrete.
///
/// With an `Element`, the engine ran the cascade centrally — a widget could not
/// get the ordering wrong because it never saw it. Writing straight into the
/// tree hands each widget the obligation to call `resolve()` *after* it has
/// declared everything a selector can match on. Get it backwards and the node
/// silently goes unstyled: no panic, no diagnostic, just a rule that does
/// nothing.
///
/// `ProgressBar` shipped with exactly that inversion in this prototype —
/// `resolve()` before its `Common` was applied — so a caller's `.class()` was
/// invisible to the cascade. This test pins the fix.
#[test]
fn a_caller_supplied_class_is_visible_to_the_cascade() {
    use lumen_widgets::ProgressBar;
    let mut s = sink(".metered { width: 640px; }");
    let (n, _) = ProgressBar::new(0.5)
        .class("metered")
        .lower(&mut s, None);
    assert_eq!(
        s.meta[&n].layout_style.width,
        lumen_layout::Dim::px(640.0),
        "a class set by the caller must reach the cascade; if this fails the \
         widget resolved before applying its Common"
    );
}


/// The one ordering mistake the type states cannot reject at compile time —
/// beginning a node and never ending it — is caught positively instead.
#[test]
#[should_panic(expected = "begun and never ended")]
fn an_unended_node_is_caught() {
    let mut s = sink("");
    // Dropping the guard unused is what `#[must_use]` warns about; a warning
    // is not an error everywhere, so the balance check is the backstop.
    drop(s.node(None, Role::Group).resolve());
    s.assert_balanced();
}

/// …and a well-formed build passes it.
#[test]
fn a_balanced_build_passes_the_check() {
    let mut s = sink("button { border-radius: 2px; }");
    let root = begin_row(&mut s, None);
    s.resolve(root);
    let (_, a) = Label::new("hello").lower(&mut s, Some(root));
    let (_, b) = Button::new("Go").lower(&mut s, Some(root));
    s.end(root, &row_style(8.0, 0.0), &[a, b], false);
    s.assert_balanced();
}
