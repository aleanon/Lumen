//! system_information — show basic OS/arch/CPU info.
use lumen_widgets::system::system_info;
use lumen_widgets::{widgets, App, BuildCx, Element};

/// Build the system-information app.
pub fn main_app() -> App {
    App::new(build)
}
fn build(cx: &mut BuildCx) -> Element {
    let _ = cx;
    let info = system_info();
    widgets::column(vec![
        widgets::text("System information").id("title"),
        widgets::text(format!("OS: {}", info.os)).id("os"),
        widgets::text(format!("Arch: {}", info.arch)).id("arch"),
        widgets::text(format!("CPUs: {}", info.cpus)).id("cpus"),
    ])
}
