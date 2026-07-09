# 08 — Example Suite Plan (iced-parity)

Goal: ship a canonical example gallery, using the **iced** framework's example
set as the target list. For each example this records what it demonstrates, the
current Lumen status, and **what is missing to build it on Linux**. The gaps roll
up into a proposed **M8 — Example Parity** milestone.

Status legend: ✅ buildable today · 🟡 needs a small addition · 🔴 needs a new
capability. "Linux" qualifier: GPU is available (RTX 4070 + lavapipe), so GPU
examples are in scope; signing/mobile/web are out of scope for these desktop
examples.

## 1. Example → Lumen status → gap (Linux)

| iced example | Demonstrates | Lumen status | What's missing on Linux |
|---|---|---|---|
| counter | local state + buttons | ✅ | nothing (= `examples/hello`) |
| todos | list CRUD, text input, checkbox, persistence | ✅ | nothing (column + text_field + checkbox + signals + AppSnapshot) |
| component | reusable local-state component | ✅ | nothing (`BuildCx` + signals are components) |
| tour | multi-page showcase | ✅ | nothing (router + widget set) |
| styling | theming a widget tree | ✅ | nothing (`.lss` + tokens + themes) |
| layout | flex/grid/absolute layouts | ✅ | nothing (Taffy wrapper) |
| scrollable | scroll containers | ✅ | nothing (`widgets::scroll`) |
| lazy | virtualized large lists | ✅ | nothing (`virtual_list`/`data_grid`) |
| slider / checkbox / toggler / radio | basic input widgets | ✅ | nothing (slider/checkbox/switch/radio) |
| pick_list | dropdown selection | 🟡 | `select` cycles; needs a **dropdown popup** (E8.2) |
| text_input / editor | text entry, full editor | ✅ | `text_field`/`text_area`/`rich_text_editor` + `TextEditor` (IME, selection) |
| tooltip | hover tooltips | 🟡 | `tooltip` exists as data; needs **hover-positioned popup** (E8.2) |
| events | inspect raw input events | ✅ | nothing (event model + agent observation) |
| multitouch | pinch/pan/rotate gestures | ✅ | nothing (`gesture::GestureRecognizer`) |
| screenshot | capture the UI | ✅ | nothing (`Headless::screenshot` / agent `ui.screenshot`) |
| visible_bounds | query a widget's viewport rect | ✅ | nothing (semantics `bounds` / `ui.getLayout`) |
| color_palette | color math + swatches | 🟡 | `Color` (Oklab) ready; the rotating **wheel needs Canvas** (E8.1) |
| svg | render an SVG asset | 🟡 | minimal `render::svg` (rect/circle/path); full SVG = larger (T6.2 PENDING) |
| gradient | gradient fills | 🟡 | display list has linear/radial/conic; needs a **widget/`.lss` surface** (E8.6) |
| progress_bar | determinate progress | 🟡 | add a trivial **ProgressBar widget** (E8.6) |
| loading_spinners | indeterminate animation | 🟡 | needs an **animated transform spinner** (layers have `Affine`; E8.6) |
| custom_widget | implement a widget | ✅ | nothing (widgets are `Element` constructors; see `widget-rating`) |
| custom_shader | a WGSL shader widget | ✅ | nothing (`ShaderWidget`, T4.1, GPU goldens) |
| custom_quad | a primitive custom draw | 🔴 | needs the **Canvas** immediate-mode draw API (E8.1) |
| arc / bezier_tool | draw + edit vector paths | 🔴 | **Canvas** (paths/arcs from app code, pointer-driven) (E8.1) |
| clock | analog clock face | 🔴 | **Canvas** (rotated hands) + `Timer` (have) (E8.1) |
| stopwatch | running timer | 🟡 | `Timer` events exist; needs a **tick driver / time source** in the loop (E8.6) |
| solar_system | animated orbits | 🔴 | **Canvas** + the motion clock (E8.1) |
| game_of_life | grid sim + interaction | 🔴 | **Canvas** (or a perf-tuned cell grid) (E8.1) |
| sierpinski_triangle | recursive vector drawing | 🔴 | **Canvas** (E8.1) |
| vectorial_text | text as scalable outlines | 🟡 | swash exposes glyph outlines; needs **outline-path rendering** (E8.1/E8.6) |
| qr_code | encode + render a QR | 🟡 | rendering = rect grid (have); needs a **QR encoder** (E8.6) |
| markdown | render markdown | 🟡 | `rich_text` + markdown-lite; needs a **markdown parser → rich elements** (E8.5) |
| changelog | scrollable markdown doc | 🟡 | depends on markdown (E8.5) |
| image | image viewer | 🟡 | PNG works; jpeg/webp/gif decode PENDING (E8.6) |
| ferris | animated sprite | 🟡 | needs **animated images (GIF/APNG)** (E8.6) |
| modal | modal dialog + backdrop | 🟡 | `stack` overlays; needs **backdrop + focus trap** (E8.2) |
| toast | transient notifications | 🟡 | overlay + **auto-dismiss animation** (E8.2) |
| menu | dropdown/context menus | 🟡 | menu model exists; needs **positioned popup menus** (E8.2) |
| combo_box | autocomplete dropdown | 🟡 | needs **popup + text filtering** (E8.2) |
| pane_grid | resizable, splittable panes | 🔴 | `split_pane` is fixed 2-pane; needs a **resizable pane grid** (E8.4) |
| pokedex | fetch + display from an API | 🔴 | needs an **async HTTP client** on `resource()` (E8.3) |
| download_progress | streamed download + progress | 🔴 | **HTTP client** + streaming (E8.3) |
| websocket | live WebSocket chat | 🔴 | needs a **WebSocket client** (agent has server-side tungstenite only) (E8.3) |
| url_handler | open external URLs / deep links | 🟡 | `nav::deep_link` ready; opening URLs needs an **OS open request + binding** (E8.7) |
| exit | programmatic window close | 🟡 | needs the **OS window binding** (winit close; model in T5.2) (E8.7) |
| system_information | CPU/mem/GPU info | 🔴 | needs a **system-info source** (sysinfo) (E8.7) |
| integration | embed Lumen in a host renderer | 🟡 | GPU renderer exists; needs a documented **embedding surface** (E8.7) |

## 2. Capability gaps → proposed M8 tasks

The mapping collapses to a handful of capabilities. Highest leverage first.

- **E8.1 — Canvas widget (keystone).** An immediate-mode 2D drawing surface: a
  `Canvas`/`canvas(draw_fn)` where app code emits paths, arcs, rects, gradients,
  text, and `Affine` transforms (the display-list primitives already exist —
  this is a public widget + a `Frame` builder + pointer hit-mapping). CPU
  goldens. *Unblocks:* custom_quad, arc, bezier_tool, clock, solar_system,
  game_of_life, sierpinski_triangle, color_palette wheel, vectorial_text.
- **E8.2 — Overlays & popups.** A layered overlay/popup positioner (anchor to a
  node, screen-edge flipping) + modal (backdrop + focus trap) + toast
  (auto-dismiss). *Unblocks:* modal, toast, menu (dropdowns), combo_box,
  pick_list, tooltip positioning.
- **E8.3 — Networking.** An async HTTP client + WebSocket client layered on
  `resource()` (needs an ADR-003 whitelist escalation: a minimal async runtime +
  http/ws client). *Unblocks:* pokedex, download_progress, websocket.
- **E8.4 — Resizable pane grid.** Draggable splitters, nested panes, drag-to-
  reorder, maximize. *Unblocks:* pane_grid.
- **E8.5 — Markdown.** A CommonMark-subset parser → `rich_text`/Element tree
  (headings, lists, code, links, inline images). *Unblocks:* markdown, changelog.
- **E8.6 — Small widgets & assets.** ProgressBar, a spinner (animated `Affine`),
  a gradient widget + `.lss` gradient syntax, a QR encoder, glyph-outline
  (vectorial) text, extra image codecs (jpeg/webp) + animated images (GIF/APNG),
  and a `stopwatch` time-tick driver. *Unblocks:* progress_bar,
  loading_spinners, gradient, qr_code, vectorial_text, image, ferris, stopwatch.
- **E8.7 — OS window & system binding.** Realize the T5.2 model on winit: real
  multi-window, programmatic exit, open-URL, and a system-info source (sysinfo).
  *Unblocks:* exit, url_handler, multi-window, system_information, integration.
- **E8.8 — The example gallery.** Build every example above as
  `examples/iced-parity/<name>` (each `just run`-able and agent-tested), plus a
  gallery index. The acceptance is: every ✅/🟡 example committed and green;
  each 🔴 example committed once its capability task lands.

## 3. Phased build order

1. **Phase 0 — zero-gap now** (no new capability): counter, todos, component,
   tour, styling, layout, scrollable, lazy, slider/checkbox/toggler/radio,
   text_input, editor, events, multitouch, screenshot, visible_bounds,
   custom_widget, custom_shader. *(Build these immediately under E8.8.)*
2. **Phase 1 — Canvas (E8.1)** then its dependents: custom_quad, arc,
   bezier_tool, clock, solar_system, game_of_life, sierpinski_triangle,
   color_palette, vectorial_text.
3. **Phase 2 — overlays/markdown/panes/small-widgets** (E8.2, E8.4, E8.5, E8.6):
   modal, toast, menu, combo_box, pick_list, tooltip, pane_grid, markdown,
   changelog, progress_bar, loading_spinners, gradient, qr_code, image, ferris,
   stopwatch.
4. **Phase 3 — networking + system** (E8.3, E8.7): pokedex, download_progress,
   websocket, url_handler, exit, system_information, integration.

Each example ships with the project's standard triple where applicable (golden +
semantics + agent-driven interaction) and a `just run <name>` entry. The single
highest-impact gap is **E8.1 (Canvas)** — it alone unblocks the largest cluster
of iced examples, and every primitive it needs is already in the display list.

## 4. Execution status (M8)

Capabilities landed (each verified on Linux, gated commits):
- **E8.1 Canvas** ☑ — `lumen_render::canvas::Frame` + `widgets::canvas`.
- **E8.2 Overlays** ☑ (partial) — `widgets_extra::modal` + toast example;
  **anchored dropdown popups landed** (`widgets::pick_list`, an anchored
  overlay with dismiss — the pattern to generalize into Popover, plan W.1);
  combo/menu/tooltip positioning on top of it still PENDING.
- **E8.3 Networking** ☑ (partial) — WebSocket client (`tungstenite`); HTTP
  fetch (pokedex/download_progress) PENDING (async runtime + HTTP client = ADR-003
  escalation).
- **E8.4 Pane grid** ☑ — `widgets_extra::pane_grid` (draggable split).
- **E8.5 Markdown** ☑ — `widgets::markdown::render` (CommonMark subset).
- **E8.6 Small widgets** ☑ (partial) — `progress_bar`, gradient
  (`Frame::linear_gradient_rect`), spinner; QR/vectorial-outline-text/extra
  codecs/animated-images PENDING.
- **E8.7 System** ☑ (partial) — `system::system_info` (OS/arch/cpus); full
  `sysinfo` + OS window binding (exit/url/multi-window/integration) PENDING.
- **E8.8 Gallery** ☑ — `examples/iced-parity` with **21 example apps**, all
  agent-tested (`cargo test -p iced-parity`): counter, todos, events, tour, clock,
  sierpinski, color_palette, progress_bar, gradient, loading_spinners, modal,
  toast, markdown, changelog, pane_grid, svg, styling, stopwatch, image,
  system_information, websocket.

Remaining iced examples map to the PENDING items above (HTTP: pokedex,
download_progress; anchored popups: combo_box, menu, pick_list dropdowns,
positioned tooltip; assets: qr_code, ferris, vectorial_text, full-codec image;
OS binding: exit, url_handler, multi-window, integration; custom_quad/arc/
bezier_tool/game_of_life/solar_system reuse the Canvas already shipped).
