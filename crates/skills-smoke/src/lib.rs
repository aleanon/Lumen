//! Skills-smoke gate (remediation plan S0.7): pins the load-bearing claims
//! and snippets of `.claude/skills/*` to the code, so a framework change
//! that invalidates a skill fails `cargo test --workspace` (and thus
//! `just check`) instead of silently rotting the documentation.
//!
//! The crate itself is empty — see `tests/skills.rs`. When a test here
//! fails, fix the skill (and any spec section, per AGENT.md's doc-currency
//! rule) in the same commit as the framework change.
