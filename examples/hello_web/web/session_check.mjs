// P.2 gate (node leg): full interactivity without a browser — boot the
// persistent session through the SAME app.mjs loader the browser uses (canvas
// stubbed), click the button via agent-resolved geometry, and assert the
// state changed through the agent bridge.
import { readFile } from 'node:fs/promises';
import { boot } from './app.mjs';

const wasmPath = process.argv[2];
const bytes = await readFile(wasmPath);
const canvas = { width: 0, height: 0, clientWidth: 400, clientHeight: 300 }; // no ctx: headless
globalThis.performance ??= { now: () => Date.now() };

const { agent, exports: ex } = await boot(bytes, canvas, { scale: 1, width: 400, height: 300 });

const rpc = (method, params = {}) =>
  JSON.parse(agent(JSON.stringify({ jsonrpc: '2.0', id: 1, method, params })));

// 1. The tree is live and the label starts at 0.
let tree = rpc('ui.getTree');
const flat = [];
(function walk(n) { flat.push(n); (n.children || []).forEach(walk); })(tree.result.root);
const label = flat.find((n) => n.id === 'hello');
if (!label.label.includes('— 0')) throw new Error(`unexpected start: ${label.label}`);

// 2. Click the button through the POINTER path (not input.click): center from
//    the agent's geometry, injected like the browser listeners do.
const tap = flat.find((n) => n.id === 'tap');
const cx = tap.bounds.x + tap.bounds.w / 2, cy = tap.bounds.y + tap.bounds.h / 2;
ex.lumen_web_pointer(0, cx, cy);
ex.lumen_web_pointer(2, cx, cy);
ex.lumen_web_frame(16);

// 3. Assert through the agent.
tree = rpc('ui.getTree');
const after = JSON.stringify(tree);
if (!after.includes('— 1')) throw new Error('pointer click did not increment');

// 4. Frame bytes flow (repaint after state change).
ex.lumen_web_pointer(0, cx, cy); ex.lumen_web_pointer(2, cx, cy);
const n = ex.lumen_web_frame(16);
if (n <= 0) throw new Error('no frame bytes after input');

console.log('web session OK: tree live, pointer→state 0→1→2, frames flow');
