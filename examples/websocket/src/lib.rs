//! websocket — a live WebSocket client styled as a chat. `echo_once` performs a
//! real `ws://` round-trip (tungstenite); the UI seeds a short conversation and
//! the composer echoes locally so the screen is deterministic offline. Chrome
//! and the message bubbles are themed from `app.lss`.
use lumen_widgets::element::Shadow;
use lumen_widgets::{widgets, App, BuildCx, Element};

use lumen_layout::{Align, Dim, Edges};

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
    App::new(build).stylesheet(include_str!("../app.lss"))
}

fn txt(s: impl Into<String>, size: f32, weight: f32) -> Element {
    let mut e = widgets::text(s);
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = size;
        ts.weight = weight;
    }
    e
}

/// One chat row: a padded bubble pushed to the left (incoming) or right
/// (outgoing) of the log.
fn bubble(outgoing: bool, text: &str) -> Element {
    let dir = if outgoing { "out" } else { "in" };
    let mut b = txt(text, 14.0, 500.0).class("bubble").class(dir);
    b.style.padding = Edges {
        left: Dim::px(13.0),
        right: Dim::px(13.0),
        top: Dim::px(9.0),
        bottom: Dim::px(9.0),
    };

    let mut row = widgets::row(vec![b]);
    row.style.width = Dim::pct(1.0);
    row.style.justify_content = Some(if outgoing { Align::End } else { Align::Start });
    row
}

fn build(cx: &mut BuildCx) -> Element {
    let log = cx.signal("log", || {
        vec![
            (false, "Connected to wss://echo.lumen.dev".to_string()),
            (true, "ping".to_string()),
            (false, "ping".to_string()),
            (true, "hello, socket".to_string()),
            (false, "hello, socket".to_string()),
        ]
    });
    let draft = cx.signal("draft", String::new);
    let lines = log.get(cx.runtime());

    // Status header: an accent dot + "ONLINE" beside the title.
    let header = {
        let mut dot = Element::default().class("dot");
        dot.style.width = Dim::px(8.0);
        dot.style.height = Dim::px(8.0);
        let mut status = widgets::row(vec![dot, txt("ONLINE", 11.0, 800.0).class("status")]);
        status.style.column_gap = Dim::px(7.0);
        status.style.align_items = Some(Align::Center);

        let mut r = widgets::row(vec![txt("WebSocket", 22.0, 800.0).class("title"), status]);
        r.style.width = Dim::pct(1.0);
        r.style.align_items = Some(Align::Center);
        r.style.justify_content = Some(Align::SpaceBetween);
        r
    };

    // Message log.
    let mut feed: Vec<Element> = lines
        .iter()
        .enumerate()
        .map(|(i, (out, t))| bubble(*out, t).id(format!("msg-{i}")))
        .collect();
    let log_col = {
        let mut c = widgets::column(std::mem::take(&mut feed));
        c.style.row_gap = Dim::px(8.0);
        c.style.width = Dim::pct(1.0);
        c
    };

    // Composer: the live input plus a Send button that echoes locally.
    let composer = {
        let field = widgets::text_field_basic(cx, "draft", "").id("draft");
        let send = widgets::button("Send", move |rt| {
            let d = draft.get(rt);
            if !d.is_empty() {
                log.update(rt, |v| {
                    v.push((true, d.clone()));
                    v.push((false, d.clone()));
                });
                draft.set(rt, String::new());
            }
        })
        .class("send")
        .id("send");
        let mut send = send;
        if let Some(ts) = send.text_style_mut() {
            ts.font_size = 14.0;
            ts.weight = 700.0;
        }
        send.style.padding = Edges {
            left: Dim::px(18.0),
            right: Dim::px(18.0),
            top: Dim::px(10.0),
            bottom: Dim::px(10.0),
        };

        let mut field = field;
        field.style.flex_grow = 1.0;

        let mut r = widgets::row(vec![field, send]);
        r.style.column_gap = Dim::px(10.0);
        r.style.width = Dim::pct(1.0);
        r.style.align_items = Some(Align::Center);
        r
    };

    let mut card = widgets::column(vec![header, log_col, composer]).id("card");
    card.style.row_gap = Dim::px(16.0);
    card.style.padding = Edges::all(Dim::px(26.0));
    card.style.width = Dim::px(420.0);
    card.style.align_items = Some(Align::Start);
    card.shadow = Some(Shadow::soft());

    let mut page = widgets::column(vec![card]).id("page");
    page.style.width = Dim::pct(1.0);
    page.style.height = Dim::pct(1.0);
    page.style.align_items = Some(Align::Center);
    page.style.justify_content = Some(Align::Center);
    page
}
