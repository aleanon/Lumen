#[derive(Default)]
struct App { count: i64 }

#[derive(Debug, Clone, Copy)]
enum Msg { Inc }

fn update(app: &mut App, msg: Msg) {
    match msg { Msg::Inc => app.count += 1 }
}

fn view(app: &App) -> iced::Element<'_, Msg> {
    iced::widget::column![
        iced::widget::text(app.count.to_string()),
        iced::widget::button("Increment").on_press(Msg::Inc),
    ]
    .into()
}

fn main() -> iced::Result {
    iced::application(App::default, update, view).title("iced-app").run()
}
