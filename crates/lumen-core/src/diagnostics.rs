//! Structured diagnostics — "errors as data" (01 §1.4).
//!
//! Every compile/runtime/reload problem is a [`Diagnostic`]: a stable
//! machine-readable `code`, a human message, and optional source/node anchors.
//! Diagnostics serialize to JSON on the agent protocol (03 §3) and print
//! human-readably on stderr.
//!
//! Codes are **stable API** (ADR-019). The authoritative registry is
//! `lumen-core/diagnostics.md`; the [`codes`] module mirrors it as constants so
//! emitters never hand-write a string literal.

use crate::identity::StableId;

/// Diagnostic severity. The leading letter of the code matches: `E` → error,
/// `W` → warning.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A warning: the app keeps running, possibly with degraded behavior.
    Warning,
    /// An error: the offending operation was rejected (e.g. a stylesheet with a
    /// parse error is dropped atomically, keeping the previous one live).
    Error,
}

/// A location in a source file (e.g. a `.lss` or `.wgsl` span). 1-based line and
/// column, matching editor conventions.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct SourceSpan {
    /// Path to the source file, as the user referred to it.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub col: u32,
}

/// A structured diagnostic. Shape is normative (02 §9).
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    /// Stable code from the registry, e.g. `"E0101"`. See [`codes`].
    pub code: &'static str,
    /// Severity; agrees with the leading letter of `code`.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
    /// Source location, when the diagnostic refers to author text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
    /// The node this diagnostic concerns, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<StableId>,
}

impl Diagnostic {
    /// Build a diagnostic from a registry `code` and a message. Severity is
    /// inferred from the code's leading letter (`E`/`W`).
    ///
    /// ```
    /// use lumen_core::{codes, Diagnostic, Severity};
    /// let d = Diagnostic::new(codes::E0102, "unknown property `colr`");
    /// assert_eq!(d.severity, Severity::Error);
    /// assert_eq!(d.code, "E0102");
    /// ```
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        let severity = match code.as_bytes().first() {
            Some(b'E') => Severity::Error,
            _ => Severity::Warning,
        };
        Diagnostic {
            code,
            severity,
            message: message.into(),
            span: None,
            node: None,
        }
    }

    /// Attach a source span (builder style).
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Attach the node this diagnostic concerns (builder style).
    pub fn with_node(mut self, node: StableId) -> Self {
        self.node = Some(node);
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(span) = &self.span {
            write!(f, " ({}:{}:{})", span.file, span.line, span.col)?;
        }
        Ok(())
    }
}

/// Stable diagnostic codes — one `const` per row of `lumen-core/diagnostics.md`.
///
/// These strings are API: never reuse or renumber (ADR-019).
pub mod codes {
    /// Duplicate [`StableId`](crate::StableId) in a window; first match wins.
    pub const W0001: &str = "W0001";
    /// Dropped unknown state field on snapshot restore.
    pub const W0002: &str = "W0002";
    /// `.lss` parse error.
    pub const E0101: &str = "E0101";
    /// Unknown style property (carries a did-you-mean suggestion).
    pub const E0102: &str = "E0102";
    /// Style value type mismatch.
    pub const E0103: &str = "E0103";
    /// Unknown `$token` reference.
    pub const E0104: &str = "E0104";
    /// Layout overflow.
    pub const W0103: &str = "W0103";
    /// Rendered ink is clipped by its own box — content (usually text) paints
    /// past the layout box, so it gets cut off (e.g. a too-small line-height
    /// clipping descenders).
    pub const W0104: &str = "W0104";
    /// An interactive node laid out with zero area — clickable but invisible /
    /// unhittable (usually a missing size or empty content).
    pub const W0105: &str = "W0105";
    /// A `.lss` property parses and is spelled correctly, but `Style::apply`
    /// does nothing with it, so the declaration has no effect (SD5.2).
    ///
    /// Reported once per declaration at parse time. Before this the rule was
    /// simply silent — the defect class an agent cannot detect, since there is
    /// no error to read and no screen to look at.
    pub const W0107: &str = "W0107";
    /// A scroll container is laying out a large number of children directly
    /// (VL1). `Scrollable` materializes every child every frame, so cost grows
    /// with the item count rather than the viewport; `VirtualList` renders only
    /// the visible window plus overscan and is flat in item count.
    ///
    /// Advisory, not an error: a long-but-bounded list is a legitimate choice.
    /// It exists because the cheap widget is the discoverable one and the
    /// scalable widget is the one you have to already know about.
    pub const W0108: &str = "W0108";
    /// A node declares a semantic `Action` it does not implement (W2). The
    /// action list is the contract the agent and assistive tech read to decide
    /// what a node can do, so advertising an unimplemented one makes the
    /// semantic tree lie — `input.invokeAction` fails and AT offers a control
    /// that does nothing.
    pub const W0106: &str = "W0106";
    /// A known, implemented `.lss` property was given a **value** the runtime
    /// cannot use, so the declaration silently does nothing (SD5.x).
    ///
    /// Distinct from [`W0107`], which reports a property that is not
    /// implemented at all. This is the other half of the same defect class and
    /// was the one still open: `text-align: justify`, `overflow: scroll` and
    /// `aspect-ratio: 0` all parse, fail their value check, and leave the
    /// property unset. Nothing reported it — not the diagnostics, and not
    /// `get_styles`, which returns the *declared* value rather than the applied
    /// one, so a rejected value still appears there.
    ///
    /// Reported once per declaration at parse time, like `W0107`: the author
    /// needs to hear it once and the hot path must not pay for it.
    pub const W0109: &str = "W0109";
    /// An element needs a rasterized sprite larger than the renderer can
    /// portably upload; the paint is degraded rather than exact.
    ///
    /// Two situations, deliberately reported under one code because the author's
    /// fix is the same — make the element smaller, or drop the effect on it:
    ///
    /// * **Portability advisory**, from `lint()` on any backend: the sprite an
    ///   element's box-shadow needs exceeds 2048 px, the WebGL2/downlevel floor.
    ///   It may render perfectly on the author's GPU and be wrong on a user's,
    ///   which is exactly the class a CPU-only test suite cannot see.
    /// * **The renderer actually clamped**, from the GPU backend: a texture,
    ///   frame or surface was downscaled or refused to fit the device limit.
    ///
    /// Before this existed, all of it was silent — an oversize shadow sprite was
    /// a hard `create_texture` panic in the live window and nothing at all in
    /// headless tests, because the CPU renderer has no texture-dimension
    /// concept.
    pub const W0110: &str = "W0110";
    /// Shader compile error.
    pub const E0201: &str = "E0201";
    /// Missing semantics on a focusable leaf (no label or value).
    pub const W0301: &str = "W0301";
    /// Text whose APCA contrast against its **composited** backdrop is below
    /// the legibility floor — the text is effectively invisible to a reader
    /// (WCAG 1.4.3 territory, measured with APCA rather than the WCAG 2.x
    /// ratio; see `lumen_render::analysis`).
    ///
    /// Deliberately a *legibility* floor, not a design opinion: the finer
    /// tiers (`ContrastLevel::BodyText`/`LargeText`/`NonText`) stay available
    /// to callers who want to grade a palette. This code fires only when the
    /// text cannot be read at all, which is the case an agent cannot see and
    /// a human sees instantly.
    ///
    /// Limitation, inherited from `resolve_backdrop`: only solid fills
    /// contribute to the composited background. Text over a gradient or an
    /// image is not assessed rather than assessed wrongly.
    pub const W0303: &str = "W0303";
    /// A deprecated `node-<index>` agent handle was accepted (ID2 alias
    /// window). The id space moved to `nx-<hex>` because arena slot indices
    /// are re-minted every rebuild, so persisting the arena makes them
    /// ambiguous. Emitted on input only — the server never *returns* the old
    /// form, since after CP6 it would be wrong rather than merely deprecated.
    pub const W0302: &str = "W0302";
    /// Missing translation for a message key in the active locale (T5.3).
    pub const W0401: &str = "W0401";
    /// Tofu: a shaped text block contains `.notdef` glyphs — characters no
    /// registered font covers (T.4; the lean Latin-subset build surfaces
    /// uncovered scripts here instead of silently drawing boxes).
    pub const W0402: &str = "W0402";
    /// A build/layout/paint panic was contained; the previous frame was kept
    /// and the app stayed alive (T7.3 error boundary, top level).
    pub const E0701: &str = "E0701";
    /// An UNcontained panic crossed the crash-report hook (E.3): the process
    /// is going down, but the structured report reached the sink first.
    pub const E0702: &str = "E0702";
}

// --- E.3: the crash-report hook -----------------------------------------------

/// Install a process-wide crash-report hook: any panic that escapes the
/// error boundaries is converted to a structured [`Diagnostic`]
/// ([`codes::E0702`], message = panic payload + location) and handed to
/// `sink` BEFORE the default panic output runs. The sink must not panic and
/// should be fast (write a file, post a channel) — the process is going
/// down. Telemetry is explicitly NOT what this is: nothing leaves the
/// machine unless the app's own sink sends it (privacy stance, 01 §ops).
pub fn install_crash_hook(sink: impl Fn(Diagnostic) + Send + Sync + 'static) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".into());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".into());
        sink(Diagnostic::new(
            codes::E0702,
            format!("uncontained panic at {location}: {payload}"),
        ));
        previous(info);
    }));
}
