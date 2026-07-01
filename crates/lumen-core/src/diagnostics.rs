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
    /// Shader compile error.
    pub const E0201: &str = "E0201";
    /// Missing semantics on a focusable leaf (no label or value).
    pub const W0301: &str = "W0301";
    /// Missing translation for a message key in the active locale (T5.3).
    pub const W0401: &str = "W0401";
    /// A build/layout/paint panic was contained; the previous frame was kept
    /// and the app stayed alive (T7.3 error boundary, top level).
    pub const E0701: &str = "E0701";
}
