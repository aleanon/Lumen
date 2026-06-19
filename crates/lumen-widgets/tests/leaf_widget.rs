//! E2: a custom leaf widget (as a third party would define it) is first-class —
//! the runtime measures it, paints it, and surfaces its semantics; the agent can
//! see it. This is the T7.2 "external widget driven unmodified" acceptance, real.

use lumen_core::geometry::Size;
use lumen_core::semantics::Role;
use lumen_core::Color;
use lumen_render::canvas::Frame;
use lumen_render::Brush;
use lumen_widgets::{widgets, App, BuildCx, LeafWidget};

/// A custom leaf defined "outside" the framework: a solid square that knows its
/// own size, how to paint, and its accessibility.
struct Swatch {
    color: Color,
    size: f64,
}

impl LeafWidget for Swatch {
    fn measure(&self, _available: kurbo::Size) -> kurbo::Size {
        kurbo::Size::new(self.size, self.size)
    }
    fn paint(&self, f: &mut Frame, size: kurbo::Size) {
        f.fill_rect(
            kurbo::Rect::new(0.0, 0.0, size.width, size.height),
            Brush::Solid(self.color),
        );
    }
    fn semantics(&self) -> (Role, String) {
        (Role::Image, "swatch".to_string())
    }
}

fn bounds_w(a: &lumen_widgets::Headless, id: &str) -> f64 {
    fn find(n: &lumen_core::semantics::SemanticsNode, id: &str) -> Option<f64> {
        if n.id.as_ref().map(|i| i.as_str()) == Some(id) {
            return Some(n.bounds.width());
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    find(&a.semantics_doc().root, id).unwrap()
}

#[test]
fn custom_leaf_is_first_class() {
    let mut a = App::new(|_cx: &mut BuildCx| {
        widgets::leaf(Swatch {
            color: Color::srgb8(0xe8, 0x1a, 0x4b, 0xff),
            size: 40.0,
        })
        .id("sw")
    })
    .run_headless(Size::new(80.0, 80.0));
    a.pump();

    // Semantics: the leaf contributed its role + label (agent-visible).
    assert!(
        a.semantics_json().to_string().contains("swatch"),
        "leaf semantics surfaced to the agent"
    );
    // Measured: the runtime sized the node from the leaf's measure().
    assert!(
        (bounds_w(&a, "sw") - 40.0).abs() < 0.5,
        "leaf measured to 40px"
    );
    // Painted: a red pixel where the swatch is.
    let img = a.screenshot();
    let i = ((10 * img.width() + 10) * 4) as usize;
    let p = img.pixels();
    assert!(
        p[i] > 200 && p[i + 1] < 90 && p[i + 2] < 110,
        "swatch painted red, got {:?}",
        &p[i..i + 4]
    );
}
