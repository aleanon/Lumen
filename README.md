# Lumen

An AI-first, cross-platform native GUI framework in Rust. Its primary user is an
AI agent: every part of the UI — structure, styling, and tests — is
deterministic, diffable text, and the semantic tree that drives accessibility is
the same tree tests and agents observe.

The full specification lives in [`.ai_docs/`](.ai_docs/) (start with
`00-HANDOFF-README.md`). Work is tracked in `.ai_docs/06-task-graph.md`.

## Workspace layout

| Crate | Role |
|---|---|
| `lumen-core` | tree, NodeIndex/SoA hot data, signals, state store, events, semantics |
| `lumen-layout` | Taffy wrapper, incremental layout |
| `lumen-render` | display list, CPU (tiny-skia) + GPU (wgpu) backends |
| `lumen-text` | parley/swash wrapper, editing, IME |
| `lumen-style` | `.lss` parser, cascade, tokens, animation scheduler |
| `lumen-widgets` | built-in widget library |
| `lumen-shell` | winit desktop shell; mobile shells (M3) |
| `lumen-test` | headless test harness, locators, snapshots, traces |
| `lumen-agent` | JSON-RPC / MCP server |
| `lumen-cli` | dev server, hot reload, emulator orchestration (`lumen` binary) |
| `lumen` | public facade — user code depends on this + `lumen-test` only |

## Development

```sh
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Toolchain is pinned in `rust-toolchain.toml`. Licensed MIT OR Apache-2.0.
