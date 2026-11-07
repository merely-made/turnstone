// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// Builds the browser host for a signed block adapter pack, like `capsule-library/build.mjs` does for
// an applet. jco and @noble/hashes are development tools, not platform dependencies; this build
// takes them from a tools directory that holds a `node_modules` installed from the pinned lock
// (`@bytecodealliance/jco` 1.37.0, `@noble/hashes` 2.4.0 in `ports/moot/examples/capsule-library`).
//
//   node build_block_adapters.mjs --tools DIR --out SITE_DIR --pack PACK.json --component GUEST.wasm
//   node build_block_adapters.mjs --tools DIR --out SITE_DIR --moot MOOT_DIR
//
// The first form is a development input: one local pack, disclosed as `moot:local-test`. The
// second takes a Moot disclosure (`ports/moot/examples/block_adapter_disclosure.rs` writes
// `MOOT_DIR/moot-disclosure.json` with its `packs/` and `components/`): every disclosed entry is
// built, and the page's review names the Moot, the collection and its revision.
//
// The site directory then holds `adapters.json` (the disclosure the page reads), the pack(s),
// the component(s), the jco output with a derivation manifest that records each generated
// file's hash, and the host modules. The page is told to disclose it with
// `?block_adapters=<site>/adapters.json`.
import { readFile, writeFile, mkdir, cp, readdir } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const args = Object.fromEntries(process.argv.slice(2).reduce((pairs, arg, i, all) => (arg.startsWith('--') ? [...pairs, [arg.slice(2), all[i + 1]]] : pairs), []));
for (const need of ['tools', 'out']) if (!args[need]) throw new Error(`--${need} is required`);
if (!args.moot && !(args.pack && args.component)) throw new Error('Pass --pack and --component, or --moot');
const tools = resolve(args.tools);
const out = resolve(args.out);
const { blake3 } = await import(pathToFileURL(join(tools, 'node_modules/@noble/hashes/blake3.js')));
const digest = bytes => Buffer.from(blake3(bytes)).toString('hex');

// Verifies one signed pack against its component and builds its browser host files under
// `dir` (relative to the site), returning the entry the page reads.
async function build(packBytes, component, dir, disclosure) {
  const pack = JSON.parse(packBytes.toString('utf8'));
  const part = pack.manifest.parts[0];
  if (pack.manifest.parts.length !== 1 || part.role !== 'wasm-component' || part.blob !== `blake3:${digest(component)}` || part.bytes !== component.length) {
    throw new Error('Build refuses a component that is not the pack’s signed part');
  }
  if (pack.manifest.requested_scopes.length !== 1 || pack.manifest.requested_scopes[0] !== 'power:block-adapter') {
    throw new Error('A block adapter pack asks for power:block-adapter and nothing else');
  }
  const at = join(out, dir);
  await mkdir(join(at, 'generated'), { recursive: true });
  await writeFile(join(at, 'pack.json'), packBytes);
  await writeFile(join(at, 'guest.wasm'), component);
  const jcoPackage = JSON.parse(await readFile(join(tools, 'node_modules/@bytecodealliance/jco/package.json'), 'utf8'));
  const bin = typeof jcoPackage.bin === 'string' ? jcoPackage.bin : jcoPackage.bin.jco;
  await promisify(execFile)(process.execPath, [join(tools, 'node_modules/@bytecodealliance/jco', bin),
    'transpile', join(at, 'guest.wasm'), '--name', 'library', '--instantiation', 'async',
    '--no-wasi-shim', '--no-nodejs-compat', '-o', join(at, 'generated')]);
  const files = {};
  for (const name of await readdir(join(at, 'generated'))) {
    if (/^library(?:\.core[0-9]*)?\.(js|wasm)$/.test(name)) {
      const bytes = await readFile(join(at, 'generated', name));
      files[name] = { hash: digest(bytes), bytes: bytes.length };
    }
  }
  const host_sources = {};
  for (const name of ['verify.mjs', 'wasi.mjs', 'runtime.mjs', 'block_worker.mjs', 'block_adapters.mjs']) {
    host_sources[name] = digest(await readFile(join(here, name)));
  }
  const bindings = JSON.stringify({ jco: jcoPackage.version, component_hash: digest(component), files, host_sources }, null, 2);
  await writeFile(join(at, 'bindings.json'), bindings);
  const packHash = digest(packBytes);
  const prefix = dir ? `${dir}/` : '';
  return {
    name: pack.manifest.name,
    pack_file: `${prefix}pack.json`, pack_hash: packHash,
    component_file: `${prefix}guest.wasm`, generated_dir: `${prefix}generated`,
    bindings_file: `${prefix}bindings.json`, bindings_hash: digest(new TextEncoder().encode(bindings)),
    disclosure: disclosure(packHash),
  };
}

await mkdir(out, { recursive: true });
await cp(join(tools, 'node_modules/@noble/hashes'), join(out, 'noble'), { recursive: true, dereference: true });
for (const name of ['verify.mjs', 'wasi.mjs', 'runtime.mjs', 'block_worker.mjs', 'block_adapters.mjs']) await cp(join(here, name), join(out, name));
const adapters = [];
if (args.moot) {
  const mootDir = resolve(args.moot);
  const disclosed = JSON.parse(await readFile(join(mootDir, 'moot-disclosure.json'), 'utf8'));
  for (const field of ['moot', 'collection', 'revision']) {
    if (typeof disclosed[field] !== 'string' || !disclosed[field]) throw new Error(`The Moot disclosure has no ${field}`);
  }
  for (const entry of disclosed.entries) {
    const packBytes = await readFile(join(mootDir, entry.pack_file));
    // The disclosed address is the pack's exact bytes; a host that served other bytes is refused here and again in review.
    if (digest(packBytes) !== entry.pack_hash) throw new Error(`Pack ${entry.name} does not match its disclosed address`);
    const component = await readFile(join(mootDir, entry.component_file));
    if (`blake3:${digest(component)}` !== `blake3:${entry.component_hash}` || component.length !== entry.component_bytes) {
      throw new Error(`Component of ${entry.name} does not match its disclosed address`);
    }
    adapters.push(await build(packBytes, component, `adapters/${entry.pack_hash}`, () => ({ moot: disclosed.moot, collection: disclosed.collection, revision: disclosed.revision, capsules: [] })));
  }
  if (adapters.length === 0) throw new Error('The Moot disclosure lists no block adapters');
} else {
  adapters.push(await build(await readFile(args.pack), await readFile(args.component), '',
    packHash => ({ moot: 'moot:local-test', collection: 'block adapters (local test)', revision: packHash, capsules: [] })));
}
await writeFile(join(out, 'adapters.json'), JSON.stringify({ adapters }, null, 2));
console.log(`Block adapter site built at ${out}; ${adapters.length} adapter(s); ${args.moot ? 'disclosed by Moot' : 'local test disclosure'}`);
