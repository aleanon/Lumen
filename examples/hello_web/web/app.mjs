// P.2: the interactive browser loader — persistent wasm session, canvas
// present, input listeners, RAF loop, and the agent bridge
// (window.lumenAgent + optional dev WebSocket).
//
// Works in a browser (module script) and under node (imported by
// session_check.mjs, which stubs the canvas).

export async function boot(wasmBytes, canvas, opts = {}) {
  const { instance } = await WebAssembly.instantiate(wasmBytes, {});
  const ex = instance.exports;
  const scale = opts.scale ?? (globalThis.devicePixelRatio || 1);
  const cssW = opts.width ?? canvas.clientWidth ?? 400;
  const cssH = opts.height ?? canvas.clientHeight ?? 300;

  ex.lumen_web_start(cssW, cssH, scale);
  canvas.width = ex.lumen_web_width();
  canvas.height = ex.lumen_web_height();
  const ctx = canvas.getContext?.("2d");

  const enc = new TextEncoder();
  const dec = new TextDecoder();
  function sendStr(s, call) {
    const bytes = enc.encode(s);
    const ptr = ex.lumen_web_alloc(bytes.length);
    new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
    return call(ptr, bytes.length);
  }

  // The agent bridge: JSON-RPC string in, JSON-RPC string out — same
  // protocol as the desktop TCP endpoint (03 §3).
  function agent(json) {
    const n = sendStr(json, (p, l) => ex.lumen_web_agent(p, l));
    const out = dec.decode(new Uint8Array(ex.memory.buffer, ex.lumen_web_reply_ptr(), n));
    dirty = true; // agent actions may change state
    return out;
  }

  let dirty = true;
  function present() {
    const n = ex.lumen_web_frame(frameDt());
    if (n > 0 && ctx) {
      const px = new Uint8ClampedArray(ex.memory.buffer, ex.lumen_web_frame_ptr(), n);
      ctx.putImageData(new ImageData(px, canvas.width, canvas.height), 0, 0);
    }
    return n;
  }
  let last = performance.now();
  function frameDt() {
    const now = performance.now();
    const dt = now - last;
    last = now;
    return dt;
  }

  // Input → the one queue. Coordinates in CSS px relative to the canvas.
  const pos = (e) => {
    const r = canvas.getBoundingClientRect?.() ?? { left: 0, top: 0 };
    return [e.clientX - r.left, e.clientY - r.top];
  };
  const NAMED = {
    Enter: 0, Escape: 1, Backspace: 2, Delete: 3, Tab: 4, " ": 5,
    ArrowLeft: 6, ArrowRight: 7, ArrowUp: 8, ArrowDown: 9,
    Home: 10, End: 11, PageUp: 12, PageDown: 13,
  };
  if (canvas.addEventListener) {
    canvas.addEventListener("pointerdown", (e) => { const [x, y] = pos(e); ex.lumen_web_pointer(0, x, y); dirty = true; });
    canvas.addEventListener("pointermove", (e) => { const [x, y] = pos(e); ex.lumen_web_pointer(1, x, y); dirty = true; });
    canvas.addEventListener("pointerup", (e) => { const [x, y] = pos(e); ex.lumen_web_pointer(2, x, y); dirty = true; });
    canvas.addEventListener("wheel", (e) => { const [x, y] = pos(e); ex.lumen_web_wheel(x, y, e.deltaX, e.deltaY); e.preventDefault(); dirty = true; }, { passive: false });
    globalThis.addEventListener?.("keydown", (e) => {
      if (e.key in NAMED) { ex.lumen_web_key(NAMED[e.key], 1, e.shiftKey ? 1 : 0, e.ctrlKey ? 1 : 0); }
      else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) { sendStr(e.key, (p, l) => ex.lumen_web_text(p, l)); }
      dirty = true;
    });
    globalThis.addEventListener?.("keyup", (e) => {
      if (e.key in NAMED) { ex.lumen_web_key(NAMED[e.key], 0, e.shiftKey ? 1 : 0, e.ctrlKey ? 1 : 0); dirty = true; }
    });
  }

  // Event-driven RAF: render when input/agent dirtied the session or the UI
  // asked for animation frames; idle otherwise.
  function loop() {
    if (dirty || ex.lumen_web_needs_frame()) {
      dirty = false;
      present();
    }
    globalThis.requestAnimationFrame?.(loop);
  }
  present();
  globalThis.requestAnimationFrame?.(loop);

  // Dev transport: ?agent=ws://127.0.0.1:9231 — each text frame is one
  // JSON-RPC request; the reply goes straight back.
  const wsUrl = opts.agentWs
    ?? (globalThis.location ? new URLSearchParams(globalThis.location.search).get("agent") : null);
  if (wsUrl && globalThis.WebSocket) {
    const ws = new WebSocket(wsUrl);
    ws.onmessage = (m) => ws.send(agent(m.data));
  }

  return { agent, present, exports: ex };
}

// Browser auto-boot: `<canvas id="lumen">` + `<script type="module" src="app.mjs">`.
if (globalThis.document) {
  const canvas = document.getElementById("lumen");
  const wasm = await (await fetch("hello_web.wasm")).arrayBuffer();
  const session = await boot(wasm, canvas);
  globalThis.lumenAgent = session.agent; // CDP/console access
}
