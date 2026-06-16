// node harness (no browser): instantiate the Lumen WASM module, call the C-ABI
// render export, and write the raw RGBA frame to a file. Verifies the framework
// runs + renders correctly under WASM. Usage: node render.mjs <wasm> <w> <h> <out>
import { readFile, writeFile } from 'node:fs/promises';
const [wasmPath, w, h, out] = process.argv.slice(2);
const W = +w, H = +h;
const bytes = await readFile(wasmPath);
const { instance } = await WebAssembly.instantiate(bytes, {});
const ptr = instance.exports.lumen_web_render(W, H);
const mem = new Uint8Array(instance.exports.memory.buffer, ptr, W * H * 4);
await writeFile(out, Buffer.from(mem));
