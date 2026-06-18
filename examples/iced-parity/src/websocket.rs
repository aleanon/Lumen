//! websocket — a live WebSocket client (E8.3). `echo_once` performs a real
//! round-trip over a `ws://` connection (tungstenite, the same client the agent
//! conformance test uses); the UI is a minimal chat log.
use lumen_widgets::{theme, widgets, App, BuildCx, Element};

/// Connect to `url`, send `msg`, and return the first reply (blocking; a real
/// app runs this off the UI thread and feeds a signal).
pub fn echo_once(url: &str, msg: &str) -> Option<String> {
    let (mut ws, _) = tungstenite::connect(url).ok()?;
    ws.send(tungstenite::Message::Text(msg.to_string())).ok()?;
    loop {
        match ws.read().ok()? {
            tungstenite::Message::Text(t) => return Some(t),
            tungstenite::Message::Close(_) => return None,
            _ => {}
        }
    }
}

/// Build the chat app.
pub fn main_app() -> App {
    App::new(build)
}

fn build(cx: &mut BuildCx) -> Element {
    theme::screen("WebSocket", body(cx))
}

fn body(cx: &mut BuildCx) -> Element {
    let log = cx.signal("log", Vec::<String>::new);
    let lines = log.get(cx.runtime());
    let draft = theme::fixed_width(
        widgets::text_field_basic(cx, "draft", "").id("draft"),
        200.0,
    );
    let mut col = vec![
        theme::heading("WebSocket chat").id("title"),
        widgets::row(vec![draft, theme::accent_button("Send", |_| {}).id("send")]),
    ];
    for (i, line) in lines.iter().enumerate() {
        col.push(theme::caption(line.clone()).id(format!("msg-{i}")));
    }
    widgets::column(col).id("root")
}
