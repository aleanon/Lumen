# Agent skills assessment — what Lumen should ship for agents to build with it

*2026-07-08. Companion to docs/review-goals-2026-07.md (goal 3: agent
verifiability) and docs/review-docs-vs-code-2026-07.md (docs↔code drift).*

## 1. Why skills, and why now

Lumen's stated primary user is an AI agent. Today the agent-facing knowledge
lives in four places of very different reliability:

| Source | Reliability | Problem |
|---|---|---|
| `.claude/skills/writing-widgets` | **High** — iterated against real runs (commits: "skill: close Step 6 gaps found by the Accordion example run") | Covers only one job (authoring a widget) |
| `.ai_docs/01–05` specs | **Low for "what works"** — the drift audit found the styling spec ~75 % parse-only, the agent spec promising ~15 nonexistent methods, the task graph over-☑'d | An agent that reads the spec *fails* (wrong selectors, missing methods, `.lss` that silently does nothing) |
| `docs/backlog.md`, `cross-platform-readiness.md` | High | Status docs, not how-to |
| Session memory / AGENT.md / justfile comments | Medium | Not shipped, not discoverable, decays |

The drift audit's core lesson applies directly: **specs describe intent;
skills must encode reality.** The `writing-widgets` skill is the proof of
concept — 321 lines whose value is precisely the deltas from what a spec
would say (`--lib` false-greens, tofu glyphs, dotted-id selector parsing,
`height`-on-text-nodes, ADR-013 handler currency, order-dependent signal
seeding). Every one of those was learned by an agent failing first. The
skill suite's job is to make each failure a one-time cost.

**Two audiences, one suite.** (a) Agents building apps *with* Lumen — the
framework's customers, who should get skills scaffolded into their project by
`lumen new`; (b) agents developing Lumen itself — who need the same skills
plus contributor-only ones. Mark each skill below [app], [framework], or
[both]. Shipping app-facing skills inside the `lumen new` template (a
`.claude/skills/` directory in the scaffold) turns "AI-first framework" from
a protocol claim into a product feature no competitor has.

## 2. What exists today

- **`writing-widgets`** [both] — canonical widget shape, state/handler rules,
  semantics vocabulary, layout gotchas, headless test pattern, example-crate
  recipe, live-window drive loop. This is the template for everything below:
  step-ordered, table-heavy, every gotcha earned, references to canonical
  files, explicit pre-commit checklist.

Everything else an agent needs is currently undocumented-as-skill: how to
start an app, which `.lss` subset actually works, how to verify beyond a
single widget, how to debug a failure, how to keep a frame within budget.

## 3. The proposed skill catalog

### P0 — the core loop (an agent cannot ship an app without these)

#### 3.1 `building-apps` [app] — *"Use when creating a Lumen application or adding a screen/feature to one"*
The missing front door. Must encode:
- Project shape: `main_app() -> App` convention, example-crate layout
  (`src/lib.rs` + headless `src/main.rs` + `examples/<name>-win.rs` +
  `app.lss`), workspace `members` registration, `just run`/`render`/`test`
  recipe contract (`[[example]]` must be `<name>-win`).
- **Import reality, not spec:** depend on `lumen-core`/`lumen-widgets`/
  `lumen-layout`/`lumen-render` `{ workspace = true }` directly (91/97
  existing examples do; the facade-only rule in 02 §11 is aspirational).
- Composition: `col!`/`row!`/`Container`, the widget catalog with the
  honest availability table (what's in lumen-widgets vs example-only vs
  missing — from the drift audit §2: no Popover/Sheet/Drawer/SearchField/
  Combobox/ColorPicker/…; Toast/Spinner/Chip live in examples).
- State: `cx.signal(name, init)` keying, `Signal::update` (in place, pure
  closures — no runtime re-entry), `cx.scope` memoization **with the
  read-inside-the-scope dependency rule**, `For`/keyed lists, serializable
  state (sorted `Vec<(K,V)>` over maps).
- App-level modules that exist and work: `forms` (Validator/form_field),
  `nav::Router` (back stack, guards, deep links), `i18n` (Locale/Catalog,
  RTL), `undo::History`, `system` (MenuModel/SystemRequest — headless model
  only, no OS wiring), `tasks`/`Resource` for async (thread-pool `Spawner`;
  **no HTTP/WS client exists** — blocking I/O on the pool is the pattern).
- Stable-id discipline: every interactive node gets `.id("...")`,
  `[a-z0-9-]` only (dotted ids parse as id+class), unique per window —
  this is what makes the app agent-verifiable later.
- Size: ~250–350 lines. Source material: `examples/counter`, `todos`,
  `settings`, widget-library memory, ADR-013.

#### 3.2 `styling-lss` [app] — *"Use when writing or editing .lss stylesheets or theming a Lumen app"*
**The highest-drift area — this skill prevents the most silent failures.**
Must encode:
- **The applied-property table** (the drift audit's §3, inverted into
  guidance): what actually reaches paint (background color, border
  shorthand, uniform border-radius, backdrop-filter, text color), what is
  applied-but-ignored (opacity, font-size/weight), and what is parse-only
  (all layout properties, gradients, shadow, transform, transitions,
  typography). Rule of thumb until the runtime catches up: **layout in
  Rust (`LayoutStyle`), colors/borders/backdrop in `.lss`**.
- What silently does nothing: `@media` (applies unconditionally), nested
  `&:hover` blocks, `transition:`/`animation:`, `:hover` (runtime state is
  `hovered`), per-side borders/padding.
- Tokens/themes that DO work: `@tokens`, `@theme light|dark`, `$token`
  resolution, `set_theme` (instant, not animated).
- Hot reload workflow: `just run-hot <name>` / `LUMEN_WATCH_LSS`, atomic
  reject keeps old sheet + E0101 with span, `ui.getStyles` to confirm a
  rule landed (canonical `{px:…}`/`#rrggbbaa` forms; only `stylesheet`
  source is reachable).
- Diagnostics: E0102 has did-you-mean; E0103 does NOT fire (type mismatches
  are silent — check `get_styles` instead); `border-width`/`border-color`
  work but trigger spurious E0102.
- Size: ~150–200 lines, dominated by the property table.
  **Must be regenerated when the styling runtime lands items from the
  49-item list** — see §5 (skill-drift gate).

#### 3.3 `verifying-apps` [both] — *"Use to verify a Lumen app or feature behaves correctly — headless tests, live-window driving, goldens, lint"*
Extract + expand `writing-widgets` Step 5–6 into the general verification
skill (widgets keeps a pointer). Must encode:
- **The verification ladder** and when each rung is enough: (1) headless
  smoke (`cargo run -p <name>` → PNG → Read it), (2) headless test
  (`TestApp` + locators + `expect` + `assert_view_coherent`), (3) golden
  (`expect_screenshot`, `LUMEN_UPDATE_GOLDENS=1`, `.actual.png` on
  mismatch — no `.diff.png` yet), (4) live window (`just run-agent`).
- lumen-test reality: which assertions retry (only `to_have_text`; others
  one-shot despite "Timeout" naming), the missing surface (no right_click/
  type_text/scroll_into_view/to_be_visible), virtual clock for animations,
  `--lib` filter false-green trap, trace files
  (`target/lumen-traces/*.trace.jsonl`, failure embeds screenshot+tree).
- Live-window protocol cheat sheet (the *implemented* method table from the
  goals review, not the 03 §3 spec): getTree/getLayout/getStyles/screenshot
  (element-zoom `{selector, scale}`)/lint/diagnostics/probe/probeRegion,
  input.click/type/key/scroll(dy)/invokeAction/drop, clipboard, setLocale,
  getDeps/whatDependsOn/lastChange. Plus the traps: **no auto-wait** (poll
  `ui.getTree` after actions), `node-N` ids are not selectors (re-derive
  `#id`/`role:text-contains(…)`), `app.perf` returns zeros (wall-clock
  around `pump` instead), port lifecycle (`nc -z` wait; `pkill -f
  "<name>-win"`; one window per port).
- Structural-over-pixels doctrine + when pixels are mandatory (the tofu
  case: semantics said `▼`, lint was clean, only the screenshot showed
  boxes).
- The reusable socket client (today: inline Python in writing-widgets —
  promote to `scripts/agent_client.py` and have the skill call it).
- Size: ~250–300 lines. This is the skill that operationalizes goal 3.

#### 3.4 `debugging-lumen` [both] — *"Use when a Lumen app misbehaves — wrong layout, stale UI, missing interaction, panic, perf regression"*
The failure-mode → tool map. Must encode:
- Symptom table: *nothing updates on click* → handler captured stale
  snapshot (ADR-013; use `stable_handler!`) or impure build; *UI stale
  after signal write* → dependency not read inside `cx.scope`; *widget
  invisible to agent/a11y* → missing role/label; *layout wrong, state
  right* → assert `node_bounds_by_id`, remember text nodes ignore
  `height`; *element unclickable* → document-order hit-testing (later
  siblings win); *keyboard goes nowhere* → focus id ≠ editor id;
  *`.lss` rule ignored* → see styling-lss applied table.
- Introspection order: `app.diagnostics` → `ui.lint` (W0103 overflow,
  W0104 clip, W0105 zero-area, W0301 unnamed-focusable, WCAG) →
  `ui.getLayout` (bounds vs ink, `clipped` flag) → `ui.getDeps`/
  `ui.whatDependsOn`/`ui.lastChange` (why did/didn't it update — idle vs
  patch vs rebuild) → trace JSONL → `assert_view_coherent` for
  incremental-vs-fresh drift.
- Which diagnostics actually fire (E0101/E0102/E0104/W0103-5/E0201/E0701;
  W0001/W0301-as-diagnostic/E0103 are defined-but-dead).
- Panic behavior: subtree `error_boundary`, top-level containment keeps
  last frame + E0701.
- Size: ~150–200 lines.

### P1 — quality and depth

#### 3.5 `writing-widgets` [both] — exists; keep + small updates
- Point Step 5–6 at `verifying-apps` once it exists (dedupe the live-drive
  recipe); add the pick_list anchored-dropdown pattern as a reference
  widget; add the "promote from example to lumen-widgets" path (Toast/
  Spinner/Chip are waiting).

#### 3.6 `lumen-performance` [both] — *"Use when an app feels slow, a bench regresses, or building list/table-heavy screens"*
- Budgets + how to measure: `scripts/perf_gate.sh`, criterion names and
  current margins, `FrameStats` from `pump` (not `app.perf`), idle
  expectations (~0 CPU, 26 ns pump).
- Authoring rules with mechanism: virtualize anything unbounded
  (`virtual_list`/`data_grid` — 1M rows ≈ 0.7–1.7 ms), `cx.scope` around
  expensive subtrees (but hover currently clears view caches — expect
  rebuilds during pointer motion until F2), paint-only bindings for
  background changes, `Signal::update` over get-modify-set, avoid text
  churn (7 ms full-raster changed frames on CPU path), release builds for
  anything visual (debug is ~35× slower).
- Size: ~120–150 lines.

#### 3.7 `lumen-data-async` [app] — *"Use when loading data, running background work, or wiring external I/O"*
- `cx.resource`/`cx.task`, `Spawner` family (Inline/Manual/ThreadPool),
  shell waker semantics, `Sink` for streams; the **no-HTTP-client** reality
  and the blocking-on-thread-pool pattern; WebSocket via tungstenite
  (client exists, see `examples/websocket`); state hydration
  (`AppSnapshot`, `run_headless_restored`).
- Could fold into `building-apps` if kept short; standalone once E8.3
  networking lands. Size: ~80–120 lines.

### P2 — situational

#### 3.8 `lumen-platforms` [both] — *"Use when targeting Android, iOS, or web"*
Honest capability matrix first (from cross-platform-readiness + drift
audit): Android = renders + tier-1 reload, **no touch/IME**; iOS =
headless render only, template not turnkey; web = CPU one-shot render,
no event loop. Then the working recipes: `source android-env.sh`,
`scripts/android_*.sh`, `just run-on web`, golden-parity legs. This skill
mostly exists to stop an agent from believing T3/T5.1 ☑s. Size: ~100.

#### 3.9 `extending-lumen` [framework] — *"Use when changing framework crates (core/render/text/style/shell)"*
Contributor guardrails: ADR-003 dependency whitelist + escalation via
decision log; the R0 golden contract (CPU byte-determinism; GPU linear-
light divergence is by-design — `exact_vs_cpu()` scenes only); damage
equivalence tests; snapshot-feature discipline (`--no-default-features`
must stay green); parley zero-copy font rule; commit-per-task; `just
check` gate; where the plans live (R4/R5/F2 status). Size: ~150.

#### 3.10 `releasing-lumen-apps` [app] — deferred
`lumen package` produces one unsigned bundle dir today; a release skill
becomes worth writing when T7.1's real packaging lands. Until then a
paragraph in `building-apps` suffices.

## 4. Skill-design rules (learned from writing-widgets + the drift audit)

1. **Encode code-reality, never spec-intent.** Every claim in a skill must
   be verified against the code at writing time; where reality diverges
   from `.ai_docs`, the skill states the divergence explicitly ("the spec
   lists X; only Y exists — use Y"). Skills are the anti-drift layer.
2. **Front-load triggers.** The `description:` must name the files/dirs and
   verbs that should fire it (writing-widgets does this well).
3. **Tables over prose; snippets must run.** Every code block should be
   paste-runnable against the current tree (the Step 6c Python client is
   the model).
4. **Gotchas are the product.** A skill earns its context cost by the
   failure-modes section. If a section could be derived by reading one
   source file, link the file instead.
5. **One job per skill, cross-linked.** Don't let `building-apps` swallow
   verification; agents load skills per task.
6. **Ship them with the framework artifact.** In-repo `.claude/skills/`
   for contributors; the same files templated into `lumen new` output for
   app authors (adjust paths). This makes the skill suite part of the
   1.0 deliverable, versioned with the code it describes.
7. **Add a skill-drift gate.** The same failure mode that hit the docs will
   hit skills. Two cheap mechanisms: (a) extract the runnable snippets
   from each SKILL.md into a `skills-smoke` test crate compiled in CI;
   (b) a checklist item in the commit discipline — "does this change
   invalidate a skill table?" (the styling-lss applied-property table and
   the verifying-apps method table are the two that will churn).

## 5. Recommended build order

| # | Skill | Priority | Why first |
|---|---|---|---|
| 1 | `verifying-apps` | P0 | Operationalizes the framework's core thesis (goal 3); most content already exists in writing-widgets §5–6 + the goals review — extraction, not research. Also unblocks skill-validation of everything else. |
| 2 | `styling-lss` | P0 | Highest silent-failure rate today (spec ~75 % aspirational); cheapest to write from the drift audit's §3 table. |
| 3 | `building-apps` | P0 | The front door; needed before Lumen is handed to any agent that didn't build it. |
| 4 | `debugging-lumen` | P0 | Converts the two reviews' failure catalog into reusable diagnosis. |
| 5 | `lumen-performance` | P1 | Budgets exist and are gated; teach the patterns that keep them green. |
| 6 | `lumen-data-async` | P1 | Small; fills the async/no-HTTP gap agents will hit in any real app. |
| 7 | `extending-lumen` | P2 | Contributors (including this agent) already have partial coverage via memory + AGENT.md. |
| 8 | `lumen-platforms` | P2 | Mostly a capability-honesty table until the shells mature. |

With #1–#4 shipped (and templated into `lumen new`), an agent that has never
seen this repo can scaffold an app, style it within the working subset,
verify it structurally and visually, and diagnose its own failures — which
is the definition of goal 3 extended from "verify" to "build".

## 6. Bottom line

One excellent skill exists; seven are missing, and four of those (verify,
style, build, debug) are prerequisites for the framework's own thesis. The
raw material for all four already exists in this repo's reviews, memory, and
the writing-widgets skill — the work is extraction and verification, not
research. The two structural recommendations beyond the catalog: **ship the
skills in the `lumen new` scaffold** (skills-as-product), and **gate skill
snippets in CI** so the suite can't drift the way the specs did.
