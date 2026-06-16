//! Host smoke check: a `.lss` background on `#screen` paints red in the headless
//! frame (isolates the tier-1 styling from device I/O).
use kurbo::Size;
use lumen::{widgets, App, BuildCx, Element};

fn hello(cx: &mut BuildCx) -> Element {
    let count = cx.signal("count", || 0i32);
    let v = count.get(cx.runtime());
    widgets::column(vec![
        widgets::text(format!("Hello Lumen — {v}")).id("hello"),
        widgets::button("Tap", move |rt| count.update(rt, |c| *c += 1)).id("tap"),
    ])
    .id("screen")
}

#[test]
fn screen_background_paints_red() {
    let mut h = App::new(hello)
        .stylesheet("#screen { background: #cc1111; }")
        .run_headless(Size::new(400.0, 200.0));
    h.pump();
    let img = h.screenshot();
    let red = img
        .pixels()
        .chunks_exact(4)
        .filter(|p| p[0] > 150 && p[1] < 90 && p[2] < 90)
        .count();
    eprintln!("red={red}");
    assert!(red > 1000, "expected red background, got {red} red px");
}
