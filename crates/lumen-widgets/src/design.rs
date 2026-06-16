//! Design import (T7.5): map a JSON design spec (a design tool's export) into a
//! Lumen `.lss` stylesheet, so design output reconciles into the styling
//! surface the agent already edits. The verifiable core is the spec→`.lss`
//! mapping; a full Figma/Sketch importer layers on it.

use serde_json::Value;

/// Convert a design spec to a `.lss` stylesheet.
///
/// Spec shape:
/// ```json
/// { "tokens": { "accent": "#1a73e8ff" },
///   "rules":  { "#title": { "color": "$accent" } } }
/// ```
pub fn spec_to_lss(spec: &Value) -> String {
    let mut out = String::new();
    if let Some(tokens) = spec.get("tokens").and_then(Value::as_object) {
        out.push_str("@tokens {");
        for (k, v) in tokens {
            out.push_str(&format!(" {k}: {};", v.as_str().unwrap_or("")));
        }
        out.push_str(" }\n");
    }
    if let Some(rules) = spec.get("rules").and_then(Value::as_object) {
        for (selector, props) in rules {
            out.push_str(selector);
            out.push_str(" {");
            if let Some(p) = props.as_object() {
                for (k, v) in p {
                    out.push_str(&format!(" {k}: {};", v.as_str().unwrap_or("")));
                }
            }
            out.push_str(" }\n");
        }
    }
    out
}
