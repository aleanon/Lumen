//! MOD7 S1: the desktop shell must accept any `PlatformConfig`.
//!
//! Before S1 it accepted exactly one. `lumen_shell::run` took a fully-defaulted
//! `App`, `impl RunExt for App` covered only that instantiation, and `ShellApp`
//! /`ShellHeadless` pinned the bundle — so every seam the runtime exposes was
//! reachable headless and nowhere else. An app that opens a window, which is
//! every real app, could not select a layout or text engine at all.
//!
//! This does not open a window: the gate is headless-CI safe, and windowing is
//! covered by the live-window gate. What it pins is the part that was broken —
//! that the shell's entry points *type-check* against a non-default bundle, and
//! that the default path still resolves without anyone naming a platform.

use lumen_widgets::app::{DefaultPlatform, PlatformConfig};
use lumen_widgets::{widgets, App, BuildCx, Element};

/// A non-default bundle. It reuses the shipped engines deliberately — the point
/// is the *type*, not the behaviour, and swapping the engines too would test
/// `platform_config.rs`'s job instead of this one.
struct CustomPlatform;
impl PlatformConfig for CustomPlatform {
    type Layout = lumen_layout::LayoutTree;
    type Text = lumen_text::TextEngine;
}

fn view(_cx: &mut BuildCx) -> Element {
    widgets::column(vec![widgets::text("hello").id("lbl")])
}

/// The compile-time claim, made executable: `run` accepts an app on a custom
/// bundle. Taking the function pointer forces the generic to instantiate for
/// `CustomPlatform` without running an event loop.
#[test]
fn run_is_generic_over_the_platform() {
    let _run_custom: fn(
        App<lumen_render::TinySkia, lumen_core::tasks::InlineSpawner, CustomPlatform>,
        kurbo::Size,
    ) = lumen_shell::run::<CustomPlatform>;
    let _run_default: fn(
        App<lumen_render::TinySkia, lumen_core::tasks::InlineSpawner, DefaultPlatform>,
        kurbo::Size,
    ) = lumen_shell::run::<DefaultPlatform>;
}

/// `RunExt` must be implemented for a custom-bundle app too, and — just as
/// important — the default `App::new(..)` must still find the impl without the
/// caller naming a platform. A `P`-generic impl that broke inference would be a
/// silent ergonomic regression for every existing app.
#[test]
fn run_ext_covers_custom_and_default_apps() {
    fn assert_runnable<T: lumen_shell::RunExt>(_: &T) {}
    assert_runnable(&App::<_, _, CustomPlatform>::with_platform(view));
    assert_runnable(&App::new(view));
}

/// The seam is worth nothing if the shell silently substitutes its own bundle,
/// so check the runtime the shell would drive actually carries the custom one.
/// `open_window_with` is the shell's own path to a `Headless`; here it is
/// exercised headlessly, which is the same generic instantiation.
#[test]
fn a_custom_bundle_survives_into_the_runtime() {
    let mut h = App::<_, _, CustomPlatform>::with_platform(view)
        .with_renderer(Box::new(lumen_render::TinySkia) as Box<dyn lumen_widgets::Renderer>)
        .with_executor(lumen_core::tasks::ThreadPoolSpawner::default())
        .run_headless(kurbo::Size::new(320.0, 200.0));
    h.pump();
    assert!(
        h.node_bounds_by_id("lbl").is_some(),
        "the shell's own renderer + executor + custom bundle combination did \
         not produce a laid-out tree"
    );
}
