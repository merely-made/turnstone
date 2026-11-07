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
//
// The site directory then holds `adapters.json` (the disclosure the page reads), the pack, the
// component, the jco output with a derivation manifest that records each generated file's hash,
// and the host modules. The page is told to disclose it with `?block_adapters=<site>/adapters.json`.
import { readFile, writeFile, mkdir, cp, readdir } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const args = Object.fromEntries(process.argv.slice(2).reduce((pairs, arg, i, all) => (arg.startsWith('--') ? [...pairs, [arg.slice(2), all[i + 1]]] : pairs), []));
for (const need of ['tools', 'out', 'pack', 'component']) if (!args[need]) throw new Error(`--${need} is required`);
const tools = resolve(args.tools);
const out = resolve(args.out);
const { blake3 } = await import(pathToFileURL(join(tools, 'node_modules/@noble/hashes/blake3.js')));
const digest = bytes => Buffer.from(blake3(bytes)).toString('hex');

const packBytes = await readFile(args.pack);
const component = await readFile(args.component);
const pack = JSON.parse(packBytes.toString('utf8'));
const part = pack.manifest.parts[0];
if (pack.manifest.parts.length !== 1 || part.role !== 'wasm-component' || part.blob !== `blake3:${digest(component)}` || part.bytes !== component.length) {
  throw new Error('Build refuses a component that is not the pack’s signed part');
}
if (pack.manifest.requested_scopes.length !== 1 || pack.manifest.requested_scopes[0] !== 'power:block-adapter') {
  throw new Error('A block adapter pack asks for power:block-adapter and nothing else');
}
await mkdir(join(out, 'generated'), { recursive: true });
await writeFile(join(out, 'pack.json'), packBytes);
await writeFile(join(out, 'guest.wasm'), component);

const jcoPackage = JSON.parse(await readFile(join(tools, 'node_modules/@bytecodealliance/jco/package.json'), 'utf8'));
const bin = typeof jcoPackage.bin === 'string' ? jcoPackage.bin : jcoPackage.bin.jco;
await promisify(execFile)(process.execPath, [join(tools, 'node_modules/@bytecodealliance/jco', bin),
  'transpile', join(out, 'guest.wasm'), '--name', 'library', '--instantiation', 'async',
  '--no-wasi-shim', '--no-nodejs-compat', '-o', join(out, 'generated')]);
const files = {};
for (const name of await readdir(join(out, 'generated'))) {
  if (/^library(?:\.core[0-9]*)?\.(js|wasm)$/.test(name)) {
    const bytes = await readFile(join(out, 'generated', name));
    files[name] = { hash: digest(bytes), bytes: bytes.length };
  }
}
const host_sources = {};
await cp(join(tools, 'node_modules/@noble/hashes'), join(out, 'noble'), { recursive: true, dereference: true });
for (const name of ['verify.mjs', 'wasi.mjs', 'runtime.mjs', 'block_worker.mjs', 'block_adapters.mjs']) {
  await cp(join(here, name), join(out, name));
  host_sources[name] = digest(await readFile(join(here, name)));
}
const bindings = JSON.stringify({ jco: jcoPackage.version, component_hash: digest(component), files, host_sources }, null, 2);
await writeFile(join(out, 'bindings.json'), bindings);
const packHash = digest(packBytes);
const config = {
  adapters: [{
    name: pack.manifest.name,
    pack_file: 'pack.json', pack_hash: packHash,
    component_file: 'guest.wasm', generated_dir: 'generated',
    bindings_file: 'bindings.json', bindings_hash: digest(new TextEncoder().encode(bindings)),
    disclosure: { moot: 'moot:local-test', collection: 'block adapters (local test)', revision: packHash, capsules: [] },
  }],
};
await writeFile(join(out, 'adapters.json'), JSON.stringify(config, null, 2));
console.log(`Block adapter site built at ${out}; component blake3:${digest(component)}; jco ${jcoPackage.version}`);
