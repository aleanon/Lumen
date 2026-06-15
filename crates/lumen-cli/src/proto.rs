//! The dev-server wire protocol (03 §4): length-prefixed JSON frames between the
//! CLI server and the running app. M1 implements the tier-1 (style/asset) subset.

use lumen_core::Diagnostic;
use serde_json::{json, Value};

/// Server → app messages.
#[derive(Clone, Debug)]
pub enum ServerMsg {
    /// Push updated stylesheet bytes for a path (tier 1).
    StyleUpdate {
        /// The `.lss` path.
        path: String,
        /// The new contents.
        bytes: String,
    },
    /// Liveness probe.
    Ping,
}

impl ServerMsg {
    /// Serialize to a JSON frame.
    pub fn to_json(&self) -> Value {
        match self {
            ServerMsg::StyleUpdate { path, bytes } => {
                json!({ "type": "style_update", "path": path, "bytes": bytes })
            }
            ServerMsg::Ping => json!({ "type": "ping" }),
        }
    }
}

/// A structured reload result (03 §3/§4 reload event payload).
#[derive(Clone, Debug)]
pub struct ReloadResult {
    /// Hot-reload tier (1 for style/asset).
    pub tier: u8,
    /// `"ok"` or `"error"`.
    pub status: &'static str,
    /// Duration in milliseconds.
    pub duration_ms: f64,
    /// Diagnostics (E0101 etc. on a rejected edit).
    pub diagnostics: Vec<Diagnostic>,
}

impl ReloadResult {
    /// Serialize to the reload-event JSON frame.
    pub fn to_json(&self) -> Value {
        json!({
            "type": "reload_result",
            "tier": self.tier,
            "status": self.status,
            "duration_ms": self.duration_ms,
            "diagnostics": self.diagnostics.iter().map(|d| json!({
                "code": d.code,
                "severity": format!("{:?}", d.severity).to_lowercase(),
                "message": d.message,
            })).collect::<Vec<_>>(),
        })
    }
}
