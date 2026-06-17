//! todos — add and toggle a list of tasks.
use lumen_widgets::{theme, widgets, widgets_m1, App, BuildCx, Element};

/// Build the todos app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::screen("Todos", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let tasks = cx.signal("tasks", Vec::<(String, bool)>::new);
    let draft = cx.signal("draft", String::new);
    let list = tasks.get(cx.runtime());

    let input = theme::fixed_width(
        widgets::text_field_basic(cx, "draft", "").id("draft"),
        200.0,
    );
    let add = theme::accent_button("Add", move |rt| {
        let t = draft.get(rt);
        if !t.trim().is_empty() {
            tasks.update(rt, |v| v.push((t.clone(), false)));
            draft.set(rt, String::new());
        }
    })
    .id("add");

    let items: Vec<Element> = list
        .iter()
        .enumerate()
        .map(|(i, (name, done))| {
            let toggle =
                widgets_m1::switch(cx, &format!("done-{i}"), name.clone()).id(format!("task-{i}"));
            let mark = if *done { "[x] " } else { "[ ] " };
            widgets::row(vec![
                widgets::text(format!("{mark}{name}")).id(format!("label-{i}")),
                toggle,
            ])
        })
        .collect();

    let mut col = vec![theme::button_row(vec![input, add])];
    col.extend(items);
    widgets::column(col).id("root")
}
