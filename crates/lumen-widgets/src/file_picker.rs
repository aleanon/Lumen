//! [`FilePicker`] (W.2) — a button that requests a native file-open dialog
//! through the portable [`SystemRequest`](crate::system::SystemRequest) seam.
//! Headless/agent runs see the request in `app.systemRequests`; the desktop
//! shell fulfils it natively once P.4 lands (until then it records too). The
//! chosen path arrives back in the `{name}.path` signal when fulfilled.

use crate::widget::impl_common;
use crate::{widgets, BuildCx, Element};
use lumen_layout::Dim;

/// A file-open button over the SystemRequest seam.
/// # Example
///
/// ```
/// use lumen_widgets::{App, FilePicker};
///
/// let app = App::new(|cx| FilePicker::new(cx, "file", "Choose file…", ["png", "jpg"]).into());
/// # lumen_widgets::doc_shot(app, 240.0, 48.0, "file_picker");
/// ```
///
/// Renders:
///
/// ![File Picker example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/file_picker.png)
///
/// The picture above is `src/doc_shots/file_picker.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct FilePicker {
    el: Element,
}

impl FilePicker {
    /// A picker labelled `label`, filtering to `filters` extensions; the
    /// fulfilled path lands in `{name}.path`.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        label: impl Into<String>,
        filters: impl IntoIterator<Item = impl Into<String>>,
    ) -> FilePicker {
        cx.signal(&format!("{name}.path"), String::new);
        let filters: Vec<String> = filters.into_iter().map(Into::into).collect();
        let reply = format!("{name}.path");
        let mut el: Element = widgets::button(label, move |rt| {
            crate::system::queue_system(
                rt,
                crate::system::SystemRequest::OpenFile {
                    filters: filters.clone(),
                    reply: reply.clone(),
                },
            );
        });
        el = el.class("file-picker");
        el.style.min_width = Dim::px(120.0);
        FilePicker { el }
    }
}

impl_common!(FilePicker);
