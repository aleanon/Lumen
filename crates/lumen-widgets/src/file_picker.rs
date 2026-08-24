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
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, FilePicker, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, FilePicker::new(cx, "file", "Choose file…", ["png", "jpg"]).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 260.0, 64.0, "file_picker");
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
    name: String,
}

impl FilePicker {
    /// A picker labelled `label`, filtering to `filters` extensions; the
    /// fulfilled path lands in `{name}.path`.
    pub fn new(
        cx: &BuildCx,
        name: &str,
        label: impl Into<crate::Text>,
        filters: impl IntoIterator<Item = impl Into<String>>,
    ) -> FilePicker {
        cx.signal(format!("{name}.path"), String::new);
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
        FilePicker {
            el,
            name: name.to_string(),
        }
    }
}

impl FilePicker {
    /// Show the chosen file beneath the button, scaled to fit `max_side`.
    ///
    /// A picker that reports only a path leaves the user with no way to tell
    /// whether they picked the right file. The bytes are read on the blocking
    /// pool (keyed on the path, so changing the selection refetches and the
    /// previous preview stays up meanwhile) and decoded through
    /// [`crate::asset::decode`], which caches — so a rebuild does not re-decode.
    ///
    /// Non-images and unreadable files render the reason instead of the picture;
    /// the picker never becomes a silent no-op.
    pub fn preview(mut self, cx: &BuildCx, max_side: f64) -> FilePicker {
        let path = cx
            .signal(format!("{}.path", self.name), String::new)
            .get(cx.runtime());
        if path.is_empty() {
            return self;
        }
        let bytes = cx.resource_blocking(
            &format!("{}.bytes", self.name),
            path.clone(),
            |p: String| std::fs::read(&p).map_err(|e| e.to_string()),
        );

        let shown = match (&bytes.value, &bytes.error) {
            (Some(b), _) => match crate::asset::decode(b) {
                Ok(img) => {
                    let (w, h) = (img.width() as f64, img.height() as f64);
                    // Fit inside the box without distorting: scale by the
                    // longer side, never up past the image's own size.
                    let k = (max_side / w.max(h)).min(1.0);
                    let mut e: Element = crate::Image::new(img).into();
                    e.style.width = Dim::px((w * k) as f32);
                    e.style.height = Dim::px((h * k) as f32);
                    e
                }
                Err(e) => note(format!("not an image Lumen can decode: {e}")),
            },
            (None, Some(e)) => note(format!("could not read {path}: {e}")),
            (None, None) => note("loading…"),
        };

        let caption = note(
            std::path::Path::new(&path)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone()),
        );
        let mut col = widgets::column(vec![self.el, shown, caption]);
        col.style.row_gap = Dim::px(8.0);
        col.style.align_items = Some(lumen_layout::Align::Center);
        self.el = col;
        self
    }
}

/// A small muted caption line.
fn note(s: impl Into<String>) -> Element {
    let mut e = widgets::text(s.into());
    if let Some(ts) = e.text_style_mut() {
        ts.font_size = 12.0;
        ts.color = lumen_core::Color::srgb8(0x6b, 0x74, 0x88, 0xff);
    }
    e
}

impl_common!(FilePicker);
