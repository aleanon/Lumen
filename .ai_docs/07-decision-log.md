# 07 — Decision Log

Every "or" in the architecture has been resolved here. Implement these as written. New local decisions you make get appended under §3; architectural questions go to §2 (escalate, don't decide).

## 1. Resolved decisions (ADRs)

| # | Decision | Rationale / notes |
|---|---|---|
| ADR-001 | **Rendering abstraction: wgpu.** Single GPU API across D3D12/Vulkan/Metal/GLES; WGSL becomes the shader language for free. | Alternatives (per-platform native, OpenGL) rejected: maintenance × shader-portability cost. |
| ADR-002 | **Two backends, CPU is the renderer of record.** tiny-skia CPU backend is deterministic and headless; goldens are exact on CPU, perceptual on GPU. | Determinism for agents beats raw speed in tests; GPU parity-tested against CPU. |
| ADR-003 | **Runtime dependency whitelist:** wgpu, winit, taffy, parley, swash, tiny-skia, lyon, kurbo, libloading, notify, serde + serde_json, smol_str, bitflags, tokio (rt for agent/dev-server only), tungstenite/tokio-tungstenite, jsonschema (dev), proptest/criterion/insta (dev). Pin minor versions at repo init; record exact versions here in §3 when locked. | Anything else = escalation. Keeps the build auditable and the binary small. |
| ADR-004 | **Layout: Taffy behind `lumen-layout` wrapper.** Extensions (baseline alignment, intrinsic sizing beyond Taffy) implemented in the wrapper, upstreamed opportunistically. | Wrapper keeps engine replaceable; no direct taffy types in public API. |
| ADR-005 | **Text: parley + swash; bundled Noto fonts for all tests; no system fonts in CI.** | Cross-platform shaping determinism; system-font rendering is a headed-mode concern. |
| ADR-006 | **Path rendering v1: lyon tessellation → GPU triangles.** Vello-style compute rasterization is an M4 *evaluation*, not a dependency. | Lyon is boring and works on GLES-class hardware; revisit when scope allows. |
| ADR-007 | **Reactivity: fine-grained signals (Solid-style), no VDOM/diffing.** Identity = call-site path + explicit keys; state in a central serializable store. | O(changed) updates; the store is what makes hot reload & traces possible. |
| ADR-008 | **Tree + SoA hybrid, not ECS.** Widget logic in a tree; per-frame hot data in parallel arrays by NodeIndex; hit-test/cull are array scans. | UI workloads are sparse + hierarchical; this captures ECS cache wins where they matter without archetype churn. |
| ADR-009 | **Semantic tree = a11y tree = locator tree = agent tree.** One schema (03 §1), `semantics()` mandatory on leaf widgets. | Prevents drift between what tests query and what the AI sees. |
| ADR-010 | **Agent protocol: JSON-RPC 2.0 over WebSocket, mirrored as MCP tools; served by the CLI dev server, proxied to the app.** | One stable endpoint regardless of where the app runs (desktop/emulator/simulator). |
| ADR-011 | **State snapshots: serde_json (self-describing, field-tagged) in dev.** Positional/binary formats (postcard, bincode) forbidden for snapshots. | Survives struct evolution across hot reloads; size is irrelevant in dev. |
| ADR-012 | **Hot reload: three tiers** (data / cdylib build()-swap / snapshot restart); old dylibs never unloaded; abi_hash mismatch auto-downgrades tier 2→3. | Unloading is the segfault factory; leaking a few MB per swap in dev is fine. |
| ADR-013 | **No closures / fn-pointers / OS handles in stored state; trait objects only via `#[state_registry]`.** Handlers re-registered each `build()`. | Hard precondition for tiers 2–3, traces, and ADR-014. |
| ADR-014 | **Any-crate hot-patching linker is a separate future project, out of v1.** Lumen's only obligation: keep the `Checkpoint` protocol (02 §4) and ADR-013 discipline stable. | Subsecond/Live++-class function patching is proven feasible but is its own engineering effort. |
| ADR-015 | **Mobile: orchestrate official emulators** (avdmanager/adb, `xcrun simctl`), never ship our own. Physical iOS devices: tier-2 hot patch unsupported (codesigning) — tier 1 and 3 only. | |
| ADR-016 | **Styling: `.lss` files + 1:1 typed Rust mirror; parity enforced by `style_parity!` test.** CSS-like syntax chosen deliberately — maximal training-data familiarity for AI authors. | |
| ADR-017 | **Colors: f32 linear-light RGBA internally, sRGB/oklch at boundaries; gradients interpolate in Oklab.** | Modern interpolation quality; matches perceptual-diff metric. |
| ADR-018 | **Async: the framework core is single-threaded per window (no Send bounds on widgets); `resource()` futures run on a tokio runtime owned by the shell, results marshaled to the UI thread.** | Massively simplifies widget authoring; parallelism lives in render/layout internals where profitable. |
| ADR-019 | **Error codes are stable API** (E####/W####, registry in `lumen-core/diagnostics.md`). | Agents pattern-match on codes; never reuse or renumber. |
| ADR-020 | **License: MIT OR Apache-2.0; `cargo-deny` enforces compatible deps.** | |

## 2. Escalation list — do NOT decide these unilaterally
Stop the affected task, write `BLOCKED.md` (options + recommendation), continue elsewhere:
1. Any new runtime dependency outside ADR-003.
2. Any change to the schemas/grammars/protocols in docs 03–05 beyond additive optional fields.
3. Public API breaking changes after a milestone exit has been tagged.
4. Adopting Vello / replacing lyon (M4 evaluation outcome).
5. Physical-device (non-simulator) iOS/Android support scope.
6. Web/WASM target (explicitly out of scope for v1; tempting, refuse).
7. Multi-window APIs beyond a single primary window per app (design exists implicitly; defer).
8. Anything touching the security posture of the agent socket (auth model, non-loopback binds).

## 3. Agent amendments & locked versions
*(Append-only. Format: date — task — decision — rationale.)*

- 2026-06-14 — T0.1 (planning) — **`RgbaImage` is a first-party type, not the `image` crate.** `02 §8` types `screenshot() -> RgbaImage`; the conventional `image::RgbaImage` is **not** in the ADR-003 whitelist. Define `lumen_render::RgbaImage { width: u32, height: u32, pixels: Vec<u8> /* RGBA8, row-major */ }`, re-exported from the `lumen` and `lumen-test` facades. PNG encode/decode for goldens uses tiny-skia's `png` feature (`Pixmap::encode_png` + a thin `png`-crate reader), already in tiny-skia's transitive closure. — Rationale: avoids adding a non-whitelisted runtime dependency while honoring the `02 §8` signature. *Watch:* if `png` is judged outside tiny-skia's transitive closure, that's an ADR-003 escalation → `BLOCKED.md`.
- 2026-06-14 — T0.1 — **MSRV / toolchain pinned to stable `1.94.0`**, edition 2021, components rustfmt+clippy (`rust-toolchain.toml`). — Rationale: rule 5 + ADR-003 require a pin at repo init; 1.94.0 is the active stable on the dev host.
- 2026-06-14 — T0.1 — **Locked dependency versions** (ADR-003 §3): kurbo 0.11.3, smol_str 0.3.6 (feat. `serde`), bitflags 2.13.0, serde 1.0.228, serde_json 1, proptest 1.11.0 (dev). Pinned once in root `[workspace.dependencies]`. — Rationale: single source of truth, auditable closure. Newer kurbo 0.13 deferred to avoid churn mid-M0.
- 2026-06-14 — T0.1 — **`Color::from_hex` returns `Result<Color, ColorParseError>`** and accepts `#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa` with optional leading `#`; `to_hex()`/`Display` emit canonical `#rrggbbaa` (04 §7). — Rationale: `02 §1` gives constructor names but not fallibility; Result is the conventional choice and round-trips all 256 byte values (tested).
- 2026-06-14 — T0.1 — **`cargo-deny` `bans.allow-wildcard-paths = true`.** — Rationale: internal workspace crates are path deps without a version requirement; cargo-deny otherwise flags them as wildcard dependencies.
- 2026-06-14 — T0.3 — **Reactive runtime keyed by a flat string identity key**, interned to a `Copy` `SignalId` so `Signal<T>` stays a copyable handle (02 §4). The spec's `BuildCx::signal(name, init)` is sugar that combines identity-path + name and delegates to this `Runtime` layer; that delegation lands with `BuildCx` in T0.9. — Rationale: lets the store + reactivity be built and tested standalone before the widget/build loop exists.
- 2026-06-14 — T0.3 — **`#[state_registry]` proc-macro deferred.** No M0 acceptance exercises trait-object state, and a proc-macro needs a crate not in the 02 §1 list. The first required proc-macro is `#[lumen::test]` (T0.9); a `lumen-macros` crate will be introduced then (amending the 02 §1 crate list, recorded here) and `#[state_registry]` added alongside it. — Rationale: avoid adding a crate before any consumer needs it.
- 2026-06-14 — T0.3 — **`Resource<T>` is minimal in M0**: the future is polled once at creation (noop waker); if it doesn't resolve immediately the resource stays `Loading` until the shell's tokio executor drives it (T0.9 / ADR-018). `Signal::update` clones via a JSON round-trip to avoid a `T: Clone` bound. — Rationale: async resources have no M0 acceptance; full polling belongs with the shell runtime.
- 2026-06-14 — T0.9 (planning) — **`lumen-test` uses a hand-rolled single-threaded `block_on`, not `tokio`.** `#[lumen::test]` bodies are `async`, but ADR-003 scopes `tokio` to "agent/dev-server only." The macro wraps the test body in a minimal cooperative executor (no waker threads; the headless app is synchronous via `Headless::pump`, and `resource()` futures are polled inside `pump`). — Rationale: keeps `tokio` out of the test harness and its dependency closure, preserving ADR-003's scoping and a small CI build.
