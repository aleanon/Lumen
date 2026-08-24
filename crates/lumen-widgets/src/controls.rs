//! Single-value input controls and small decorative elements: icons, switches,
//! steppers, radios, text areas, avatars and skeleton placeholders.
//!
//! (SD2: regrouped out of the milestone-named `widgets_m*`/`misc_w2` modules,
//! which recorded WHEN a widget was written rather than what it is.)

use crate::widget::{impl_widget, Common, Widget};
use crate::{widgets, BuildCx, Canvas, Element};
use lumen_core::events::{Key, NamedKey};
use lumen_core::semantics::{Action, Role, State as SemState};
use lumen_core::Color;
use lumen_layout::{Align as LAlign, Dim, Display, Edges, FlexDirection, LayoutStyle, Position};
use lumen_text::TextStyle;
use std::rc::Rc;

/// A loading placeholder block: a soft grey box that pulses (opacity keyed
/// to the clock) while content loads.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Skeleton, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Skeleton::new(cx, 160.0, 16.0).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 200.0, 56.0, "skeleton");
/// ```
///
/// Renders:
///
/// ![Skeleton example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/skeleton.png)
///
/// The picture above is `src/doc_shots/skeleton.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Skeleton {
    width: f64,
    height: f64,
    /// The pulse alpha, sampled in `new`.
    ///
    /// Reading the animation clock registers a dependency, so it has to happen
    /// where the `BuildCx` is — deferring it to `build` would move the edge.
    alpha: u8,
    common: Common,
}

impl Skeleton {
    /// A pulsing placeholder of the given size.
    pub fn new(cx: &BuildCx, width: f64, height: f64) -> Skeleton {
        cx.animate();
        let t = cx.now_ms() / 1000.0;
        // 0.55..0.95 alpha pulse.
        let a = 0.75 + 0.20 * (t * 2.2).sin();
        Skeleton {
            width,
            height,
            alpha: (a * 255.0) as u8,
            common: Common::default(),
        }
    }
}

impl Widget for Skeleton {
    fn build(self) -> Element {
        let Skeleton {
            width,
            height,
            alpha,
            common,
        } = self;
        let mut el = Element {
            role: Role::Generic,
            background: Some(Color::srgb8(0xd7, 0xdb, 0xe1, alpha)),
            corner_radius: 6.0,
            classes: vec!["skeleton".to_string()],
            style: LayoutStyle {
                width: Dim::px(width as f32),
                height: Dim::px(height as f32),
                ..LayoutStyle::default()
            },
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Skeleton);

/// A round avatar showing the initials of a name over a color hashed from it.
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Avatar, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Avatar::new("Ada Lovelace", 40.0).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 72.0, 72.0, "avatar");
/// ```
///
/// Renders:
///
/// ![Avatar example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/avatar.png)
///
/// The picture above is `src/doc_shots/avatar.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Avatar {
    name: String,
    diameter: f64,
    /// A picture to show instead of the initials.
    img: Option<lumen_render::RgbaImage>,
    common: Common,
}

impl Avatar {
    /// An avatar of `diameter` px for `name` (initials + stable hash color).
    pub fn new(name: &str, diameter: f64) -> Avatar {
        Avatar {
            name: name.to_string(),
            diameter,
            img: None,
            common: Common::default(),
        }
    }

    /// Show `img` instead of the initials, clipped to the avatar's circle.
    ///
    /// Initials remain the **fallback** — the framework-agnostic contract for an
    /// avatar (Flutter's `CircleAvatar.backgroundImage`, Material's avatar) — so
    /// a failed or absent image still renders something identifiable, and the
    /// accessible label stays the person's name either way.
    pub fn image(mut self, img: lumen_render::RgbaImage) -> Avatar {
        self.img = Some(img);
        self
    }
}

impl Widget for Avatar {
    fn build(self) -> Element {
        let Avatar {
            name,
            diameter,
            img,
            common,
        } = self;
        let hash: u32 = name
            .bytes()
            .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
        let palette = [
            Color::srgb8(0x1a, 0x73, 0xe8, 0xff),
            Color::srgb8(0x18, 0x8a, 0x42, 0xff),
            Color::srgb8(0xc9, 0x5b, 0x0b, 0xff),
            Color::srgb8(0x8e, 0x24, 0xaa, 0xff),
            Color::srgb8(0xd3, 0x2f, 0x2f, 0xff),
            Color::srgb8(0x00, 0x83, 0x8f, 0xff),
        ];
        let bg = palette[(hash as usize) % palette.len()];
        let d = Dim::px(diameter as f32);

        // The picture path skips the initials entirely rather than building them
        // and throwing them away, which is what the eager version had to do:
        // `.image()` arrived after `new()` had already shaped the text.
        let (children, clip) = match img {
            Some(img) => {
                let mut pic: Element = crate::Image::new(img).into();
                pic.style.width = d;
                pic.style.height = d;
                pic.elide_semantics = true; // the avatar itself carries the label
                (vec![pic], true)
            }
            None => {
                let initials: String = name
                    .split_whitespace()
                    .filter_map(|w| w.chars().next())
                    .take(2)
                    .collect::<String>()
                    .to_uppercase();
                let mut text = widgets::text(if initials.is_empty() {
                    "?".to_string()
                } else {
                    initials
                });
                if let Some(ts) = text.text_style_mut() {
                    ts.font_size = (diameter * 0.4) as f32;
                    ts.weight = 600.0;
                    ts.color = Color::srgb8(0xff, 0xff, 0xff, 0xff);
                }
                (vec![text], false)
            }
        };

        let mut el = Element {
            role: Role::Image,
            label: name,
            background: Some(bg),
            corner_radius: diameter / 2.0,
            clip,
            classes: vec!["avatar".to_string()],
            style: LayoutStyle {
                width: d,
                height: d,
                align_items: Some(LAlign::Center),
                justify_content: Some(LAlign::Center),
                ..LayoutStyle::default()
            },
            children,
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Avatar);

/// A radio button in the group keyed by `group`; selecting it sets the group to
/// `value`. Exactly one member of a group is checked.
pub fn radio(cx: &BuildCx, group: &str, value: usize, label: impl Into<crate::Text>) -> Element {
    let selected = cx.signal(group, || 0usize);
    let on = selected.get(cx.runtime()) == value;
    let label = label.into();
    let marker = if on { "◉" } else { "○" };
    let (shown, shown_dyn) = label
        .clone()
        .map(move |l| format!("{marker} {l}"))
        .into_parts();
    Element {
        role: Role::Radio,
        label: label.as_static().unwrap_or_default().to_string(),
        dyn_text: shown_dyn,
        focusable: true,
        actions: vec![Action::Click, Action::Focus],
        states: if on {
            vec![SemState::Checked]
        } else {
            vec![SemState::Unchecked]
        },
        style: LayoutStyle {
            padding: Edges::all(Dim::px(4.0)),
            ..LayoutStyle::default()
        },
        content: crate::NodeContent::Text(shown, TextStyle::default()),
        on_click: Some(Rc::new(move |rt| selected.set(rt, value))),
        ..Element::default()
    }
}

/// A multi-line text input. `name` keys the text; typing (including newlines)
/// appends to it.
pub fn text_area(cx: &BuildCx, name: &str, initial: &str) -> Element {
    let value = cx.signal(name, || initial.to_string());
    let v = value.get(cx.runtime());
    let shown = if v.is_empty() {
        " ".to_string()
    } else {
        v.clone()
    };
    Element {
        role: Role::TextInput,
        focusable: true,
        label: v.clone(),
        value: Some(v),
        actions: vec![Action::Focus, Action::SetValue],
        background: Some(Color::srgb8(0xf2, 0xf2, 0xf2, 0xff)),
        corner_radius: 4.0,
        style: LayoutStyle {
            padding: Edges::all(Dim::px(6.0)),
            min_width: Dim::px(160.0),
            min_height: Dim::px(72.0),
            ..LayoutStyle::default()
        },
        content: crate::NodeContent::Text(shown, TextStyle::default()),
        on_text: Some(Rc::new(move |rt, t| {
            let t = t.to_string();
            value.update(rt, |s| s.push_str(&t))
        })),
        ..Element::default()
    }
    .id(name)
}

/// [`Icon`] — a small vector-glyph icon (Flutter `Icon` structure; typed form
/// of [`icon`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Icon, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Icon::new("gear").into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 64.0, 64.0, "icon");
/// ```
///
/// Renders:
///
/// ![Icon example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/icon.png)
///
/// The picture above is `src/doc_shots/icon.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Icon {
    label: String,
    common: Common,
}

impl Icon {
    /// An icon drawn as a vector glyph (Flutter `Icon` structure). `label` picks
    /// the symbol — `gear`/`settings`, `home`, `search`, `check`, `plus`/`add`,
    /// `close`, `menu` — and any other name falls back to a star. The label is
    /// also the accessible name.
    pub fn new(label: &str) -> Icon {
        Icon {
            label: label.to_string(),
            common: Common::default(),
        }
    }
}

impl Widget for Icon {
    fn build(self) -> Element {
        let Icon { label, common } = self;
        let color = Color::srgb8(0x33, 0x37, 0x3d, 0xff);
        let name = label.to_lowercase();
        let mut el: Element =
            Canvas::new(26.0, 26.0, move |f, sz| draw_icon(f, &name, sz, color)).into();
        el.label = label;
        common.apply(&mut el);
        el
    }
}

impl_widget!(Icon);

/// Paint a simple vector glyph named by `name` into the icon square.
fn draw_icon(
    f: &mut lumen_render::canvas::Frame,
    name: &str,
    size: lumen_core::geometry::Size,
    color: Color,
) {
    use kurbo::{BezPath, Circle, Point, Rect, Shape};
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    let s = size.width.min(size.height);
    let (cx, cy) = (size.width / 2.0, size.height / 2.0);
    let r = s * 0.40;
    let line = s * 0.10;
    let dot = |i: usize, n: usize, big: f64, small: f64, tilt: f64| {
        let rr = if i.is_multiple_of(2) { big } else { small };
        let a = tilt + i as f64 * PI / (n as f64 / 2.0);
        Point::new(cx + rr * a.cos(), cy + rr * a.sin())
    };
    match name {
        "search" => {
            let lens = Point::new(cx - s * 0.06, cy - s * 0.06);
            let lr = s * 0.24;
            f.stroke(&Circle::new(lens, lr).to_path(0.1), color, line);
            let mut h = BezPath::new();
            h.move_to(Point::new(
                lens.x + lr * FRAC_PI_4.cos(),
                lens.y + lr * FRAC_PI_4.sin(),
            ));
            h.line_to(Point::new(cx + s * 0.34, cy + s * 0.34));
            f.stroke(&h, color, line);
        }
        "check" | "done" => {
            let mut p = BezPath::new();
            p.move_to(Point::new(cx - r * 0.75, cy));
            p.line_to(Point::new(cx - r * 0.1, cy + r * 0.6));
            p.line_to(Point::new(cx + r * 0.85, cy - r * 0.55));
            f.stroke(&p, color, line * 1.1);
        }
        "plus" | "add" => {
            f.fill_rounded_rect(
                Rect::new(cx - line * 0.6, cy - r, cx + line * 0.6, cy + r),
                line * 0.5,
                color,
            );
            f.fill_rounded_rect(
                Rect::new(cx - r, cy - line * 0.6, cx + r, cy + line * 0.6),
                line * 0.5,
                color,
            );
        }
        "close" | "x" => {
            let mut a = BezPath::new();
            a.move_to(Point::new(cx - r, cy - r));
            a.line_to(Point::new(cx + r, cy + r));
            let mut b = BezPath::new();
            b.move_to(Point::new(cx + r, cy - r));
            b.line_to(Point::new(cx - r, cy + r));
            f.stroke(&a, color, line);
            f.stroke(&b, color, line);
        }
        "menu" => {
            for i in -1..=1 {
                let yy = cy + i as f64 * s * 0.22;
                f.fill_rounded_rect(
                    Rect::new(cx - r, yy - line * 0.5, cx + r, yy + line * 0.5),
                    line * 0.5,
                    color,
                );
            }
        }
        "home" => {
            let mut p = BezPath::new();
            p.move_to(Point::new(cx, cy - r));
            p.line_to(Point::new(cx + r, cy + r * 0.1));
            p.line_to(Point::new(cx + r * 0.66, cy + r * 0.1));
            p.line_to(Point::new(cx + r * 0.66, cy + r * 0.85));
            p.line_to(Point::new(cx - r * 0.66, cy + r * 0.85));
            p.line_to(Point::new(cx - r * 0.66, cy + r * 0.1));
            p.line_to(Point::new(cx - r, cy + r * 0.1));
            p.close_path();
            f.fill(&p, color);
        }
        "gear" | "settings" => {
            let teeth = 8;
            let mut p = BezPath::new();
            for i in 0..teeth * 2 {
                let pt = dot(i, teeth * 2, r, r * 0.74, 0.0);
                if i == 0 {
                    p.move_to(pt);
                } else {
                    p.line_to(pt);
                }
            }
            p.close_path();
            f.stroke(&p, color, line * 0.8);
            f.stroke(
                &Circle::new(Point::new(cx, cy), r * 0.36).to_path(0.1),
                color,
                line * 0.8,
            );
        }
        _ => {
            // Default: a filled five-point star — a generic "icon" glyph.
            let mut p = BezPath::new();
            for i in 0..10 {
                let pt = dot(i, 10, r, r * 0.42, -FRAC_PI_2);
                if i == 0 {
                    p.move_to(pt);
                } else {
                    p.line_to(pt);
                }
            }
            p.close_path();
            f.fill(&p, color);
        }
    }
}

/// A small vector-glyph icon (see [`Icon::new`] for the recognised names).
/// *(Thin shim over [`Icon`] — the typed form is preferred.)*
pub fn icon(label: &str) -> Element {
    Icon::new(label).into()
}

/// [`Switch`] — a labelled toggle switch; boolean state under `name`
/// (typed form of [`switch`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Switch, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Switch::new(cx, "wifi", "Wi-Fi").into())
/// }
/// # let app = App::new(build);
/// # // Rendered on (`wifi`).
/// # lumen_widgets::doc_shot_open(app, 140.0, 52.0, "switch", "wifi");
/// ```
///
/// Renders:
///
/// ![Switch example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/switch.png)
///
/// The picture above is `src/doc_shots/switch.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Switch {
    label: crate::Text,
    /// The signal's current value, read where the `BuildCx` is.
    checked: bool,
    /// The handle the click handler toggles (`Copy`, eight bytes).
    signal: lumen_core::state::Signal<bool>,
    common: Common,
}

impl Switch {
    /// A toggle switch with its own boolean state (`name`).
    pub fn new(cx: &BuildCx, name: &str, label: impl Into<crate::Text>) -> Switch {
        let signal = cx.signal(name, || false);
        Switch {
            label: label.into(),
            checked: signal.get(cx.runtime()),
            signal,
            common: Common::default(),
        }
    }
}

impl Widget for Switch {
    fn build(self) -> Element {
        let Switch {
            label,
            checked: is,
            signal,
            common,
        } = self;
        // The knob is what makes a switch read as a switch: it sits left
        // when off and right when on, so the state is legible without
        // relying on the track colour alone (which fails for the ~8% of men
        // with a red-green deficiency, and in any monochrome capture).
        const TRACK_W: f64 = 36.0;
        const TRACK_H: f64 = 20.0;
        const KNOB: f64 = 16.0;
        const KNOB_PAD: f64 = (TRACK_H - KNOB) / 2.0;
        let knob = Element {
            background: Some(Color::WHITE),
            corner_radius: KNOB / 2.0,
            shadow: Some(crate::element::Shadow::soft()),
            style: LayoutStyle {
                position: Position::Absolute,
                inset: Edges {
                    left: Dim::px(if is {
                        (TRACK_W - KNOB - KNOB_PAD) as f32
                    } else {
                        KNOB_PAD as f32
                    }),
                    top: Dim::px(KNOB_PAD as f32),
                    ..Edges::AUTO
                },
                width: Dim::px(KNOB as f32),
                height: Dim::px(KNOB as f32),
                ..LayoutStyle::default()
            },
            ..Element::default()
        }
        .part("knob");
        let track = Element {
            background: Some(if is {
                Color::srgb8(0x1a, 0x73, 0xe8, 0xff)
            } else {
                Color::srgb8(0xcc, 0xcc, 0xcc, 0xff)
            }),
            corner_radius: TRACK_H / 2.0,
            style: LayoutStyle {
                position: Position::Relative,
                width: Dim::px(TRACK_W as f32),
                height: Dim::px(TRACK_H as f32),
                flex_shrink: 0.0,
                ..LayoutStyle::default()
            },
            children: vec![knob],
            ..Element::default()
        }
        .part("track");
        let (label_s, label_dyn) = label.clone().into_parts();
        let mut el = Element {
            role: Role::Switch,
            label: label_s,
            dyn_text: label_dyn,
            focusable: true,
            actions: vec![Action::Click, Action::Focus],
            states: vec![if is {
                SemState::Checked
            } else {
                SemState::Unchecked
            }],
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Dim::px(6.0),
                ..LayoutStyle::default()
            },
            on_click: Some(Rc::new(move |rt| signal.update(rt, |v| *v = !*v))),
            children: vec![track, Element::text(label)],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Switch);

/// A toggle switch with its own boolean state (`name`).
/// *(Thin shim over [`Switch`] — the typed form is preferred.)*
pub fn switch(cx: &BuildCx, name: &str, label: impl Into<crate::Text>) -> Element {
    Switch::new(cx, name, label).into()
}

/// [`Stepper`] — a `-`/value/`+` numeric stepper; integer state under
/// `name` (typed form of [`stepper`]).
/// # Example
///
/// ```
/// # use lumen_widgets::App;
/// use lumen_widgets::{centered, Stepper, BuildCx, Element};
///
/// fn build(cx: &mut BuildCx) -> Element {
///     centered(cx, Stepper::new(cx, "qty", 0, 10).into())
/// }
/// # let app = App::new(build);
/// # lumen_widgets::doc_shot(app, 140.0, 64.0, "stepper");
/// ```
///
/// Renders:
///
/// ![Stepper example render](https://raw.githubusercontent.com/aleanon/Lumen/main/crates/lumen-widgets/src/doc_shots/stepper.png)
///
/// The picture above is `src/doc_shots/stepper.png` — this exact example's
/// output. `doc_shot` re-renders it every test run and fails if the render
/// drifts from that committed image, so the picture is always current.
pub struct Stepper {
    /// Kept because the child ids are namespaced under it (W4).
    name: String,
    min: i64,
    max: i64,
    /// The signal's current value, read where the `BuildCx` is.
    value: i64,
    signal: lumen_core::state::Signal<i64>,
    common: Common,
}

impl Stepper {
    /// A numeric stepper (`-`/value/`+`) with its own integer state (`name`).
    pub fn new(cx: &BuildCx, name: &str, min: i64, max: i64) -> Stepper {
        let signal = cx.signal(name, || min);
        Stepper {
            name: name.to_string(),
            min,
            max,
            value: signal.get(cx.runtime()),
            signal,
            common: Common::default(),
        }
    }
}

impl Widget for Stepper {
    fn build(self) -> Element {
        let Stepper {
            name,
            min,
            max,
            value: v,
            signal: value,
            common,
        } = self;
        // W4: child ids are namespaced under `name` — hardcoded "dec"/"inc"/
        // "value" made two steppers on one screen collide (W0001), and the
        // agent would drive whichever came first.
        let dec = crate::widgets::button("-", move |rt| value.update(rt, |x| *x = (*x - 1).max(min)))
            .id(format!("{name}-dec"));
        let inc = crate::widgets::button("+", move |rt| value.update(rt, |x| *x = (*x + 1).min(max)))
            .id(format!("{name}-inc"));
        let mut el = Element {
            role: Role::Group,
            label: format!("{v}"),
            value: Some(format!("{v}")),
            actions: vec![Action::Increment, Action::Decrement, Action::SetValue],
            focusable: true,
            style: LayoutStyle {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Dim::px(8.0),
                ..LayoutStyle::default()
            },
            // W2: honour the declared actions.
            on_increment: Some(Rc::new(move |rt| {
                value.update(rt, |x| *x = (*x + 1).min(max))
            })),
            on_decrement: Some(Rc::new(move |rt| {
                value.update(rt, |x| *x = (*x - 1).max(min))
            })),
            on_set_value: Some(Rc::new(move |rt, s| {
                if let Ok(n) = s.parse::<i64>() {
                    value.set(rt, n.clamp(min, max));
                }
            })),
            // W3: arrow keys adjust, Home/End jump to the bounds.
            on_key: Some(Rc::new(move |rt, ke| match ke.key {
                Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowRight) => {
                    value.update(rt, |x| *x = (*x + 1).min(max))
                }
                Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowLeft) => {
                    value.update(rt, |x| *x = (*x - 1).max(min))
                }
                Key::Named(NamedKey::Home) => value.set(rt, min),
                Key::Named(NamedKey::End) => value.set(rt, max),
                _ => {}
            })),
            children: vec![
                dec,
                Element::text(format!("{v}")).id(format!("{name}-value")),
                inc,
            ],
            ..Element::default()
        };
        common.apply(&mut el);
        el
    }
}

impl_widget!(Stepper);

/// A numeric stepper (`-`/value/`+`) with its own integer state (`name`).
/// *(Thin shim over [`Stepper`] — the typed form is preferred.)*
pub fn stepper(cx: &BuildCx, name: &str, min: i64, max: i64) -> Element {
    Stepper::new(cx, name, min, max).into()
}

/// The typed-vs-shim migration contract. It deliberately spans modules — Switch,
/// Stepper and Icon live here, Tabs in `nav_chrome`, VirtualList in `lists` —
/// because the contract is about the typed forms as a family, not about any one
/// module. Every type is reached through the crate root, which is also the path
/// a consumer uses.
#[cfg(test)]
mod typed_tests {
    use crate::{widgets, App};
    use kurbo::Size;
    use lumen_core::events::{Event, PointerEvent};
    use lumen_core::geometry::Point;
    use lumen_core::state::Signal;

    /// The typed forms produce the same trees as their fn shims (migration
    /// contract) and behave: Switch toggles, Tabs select, VirtualList windows.
    #[test]
    fn typed_forms_match_shims_and_behave() {
        let mut h = App::new(|cx| {
            crate::widgets::column(vec![
                crate::Switch::new(cx, "wifi", "Wi-Fi").id("sw").into(),
                crate::Tabs::new(cx, "tab", &["One", "Two"])
                    .id("tabs")
                    .into(),
                crate::Stepper::new(cx, "n", 0, 5).id("st").into(),
                crate::Icon::new("gear").id("ic").into(),
                crate::VirtualList::new(cx, "vl", 1000, 20.0, 100.0, |i| {
                    crate::widgets::text(format!("row {i}"))
                })
                .id("vl")
                .into(),
            ])
        })
        .run_headless(Size::new(400.0, 400.0));
        h.pump();

        // Switch: click toggles the boolean.
        let b = h.node_bounds_by_id("sw").expect("switch laid out");
        let p = Point::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        h.inject(Event::PointerDown(PointerEvent::at(p)));
        h.inject(Event::PointerUp(PointerEvent::at(p)));
        h.pump();
        let on: Signal<bool> = h.runtime().signal("wifi", || false);
        assert!(on.get(h.runtime()), "switch toggled");

        // VirtualList windows: ~7 rows materialized of 1000.
        let t = h.semantics_json().to_string();
        assert!(t.contains("row 0") && !t.contains("row 500"), "windowing");

        h.assert_view_coherent();
    }

    /// Shim output ≡ typed output (byte-identical semantic trees).
    #[test]
    fn shim_and_typed_trees_are_identical() {
        let a = App::new(|cx| crate::widgets::column(vec![widgets::switch(cx, "s", "L")]))
            .run_headless(Size::new(200.0, 100.0))
            .semantics_json()
            .to_string();
        let b =
            App::new(|cx| crate::widgets::column(vec![crate::Switch::new(cx, "s", "L").into()]))
                .run_headless(Size::new(200.0, 100.0))
                .semantics_json()
                .to_string();
        assert_eq!(a, b);
    }
}
