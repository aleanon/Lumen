use lumen_core::geometry::Size;
use lumen_core::state::Signal;
use lumen_widgets::{bind, widgets, App, BuildCx};

fn view(cx: &mut BuildCx) -> lumen_widgets::Element {
    let count: Signal<i64> = cx.signal("count", || 0);
    widgets::column(vec![
        widgets::text(bind!(rt => {
            let c: Signal<i64> = rt.signal("count", || 0i64);
            c.get(rt).to_string()
        })),
        widgets::button("Increment", move |rt| count.set(rt, count.get(rt) + 1)),
    ])
}

fn main() {
    use lumen_shell::RunExt;
    App::new(view).run(Size::new(400.0, 300.0));
}
