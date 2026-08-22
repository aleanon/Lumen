use xilem::view::{button, flex, label};
use xilem::{WidgetView, Xilem};

struct App { count: i64 }

fn view(app: &mut App) -> impl WidgetView<App> + use<> {
    flex((
        label(app.count.to_string()),
        button("Increment", |app: &mut App| app.count += 1),
    ))
}

fn main() -> Result<(), xilem::winit::error::EventLoopError> {
    let event_loop = xilem::EventLoop::with_user_event();
    Xilem::new(App { count: 0 }, view).run_windowed(event_loop, "xilem-app".into())
}
