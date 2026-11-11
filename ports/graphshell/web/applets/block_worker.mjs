// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// Worker containment for a guest block adapter (the `block-guest` world). It re-verifies the
// signed pack and the exact component bytes, loads the locally generated jco adapter only after
// its derivation manifest matches the hash this host pinned, and answers the pane's calls
// (`describe`, `rows`, `choices`, `apply`, `run`, `view`). The component is a pure function: it
// imports nothing but a WASI floor that supplies no files, network, clock or input. This
// worker holds no capability and emits no action; a turn that runs too long is ended by
// the host's deadline (`runtime.mjs`) and the worker is terminated.
//
// The wire mirrors `WorkerAdapter` in `block_editing/worker.rs`: requests and replies are JSON,
// with snake_case keys; the jco objects use camelCase, so this file converts both ways, and the
// view's two byte payloads cross as base64 text.

import { digest, verifyPack, verifyPart, fetchBytes } from './verify.mjs';
import { wasi } from './wasi.mjs';

let guest;
let logs = [];

const b64 = bytes => {
  let text = '';
  for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(text);
};
const attrsOut = list => list.map(({ key, value }) => ({ key, value }));
const attrsIn = list => (list || []).map(({ key, value }) => ({ key, value }));
const field = f => (f ? { label: f.label, default: f.default } : null);

const blockIn = b => ({
  class: b.class, body: b.body, attrs: attrsIn(b.attrs), revision: b.revision,
  selected: Uint32Array.from(b.selected || []), pattern: b.pattern,
});
const changeIn = c => ({
  index: c.index, label: c.label, before: c.before ?? undefined, after: c.after,
  conflict: !!c.conflict, useUpdate: !!c.use_update,
});
const changeOut = c => ({
  index: c.index, label: c.label, before: c.before ?? null, after: c.after,
  conflict: c.conflict, use_update: c.useUpdate,
});
const claimOut = c => ({
  class: c.class, source_key: c.sourceKey, source_versions: Array.from(c.sourceVersions),
  pattern: field(c.pattern),
  commands: c.commands.map(m => ({ id: m.id, label: m.label, kind: m.kind, arg: field(m.arg) })),
  views: c.views.map(v => ({ id: v.id, label: v.label })),
  example: c.example ? { title: c.example.title, body: c.example.body, attrs: attrsOut(c.example.attrs) } : null,
});

const handlers = {
  describe: () => claimOut(guest.blockAdapter.describe()),
  rows: r => guest.blockAdapter.rows(blockIn(r.block)).map(({ index, label }) => ({ index, label })),
  choices: r => guest.blockAdapter.choices(blockIn(r.block), r.command).map(changeOut),
  apply: r => attrsOut(guest.blockAdapter.apply(blockIn(r.block), r.command, r.changes.map(changeIn))),
  run: r => {
    const out = guest.blockAdapter.run(blockIn(r.block), r.command, r.arg ?? '');
    return out.tag === 'attrs' ? { attrs: attrsOut(out.val) } : { text: out.val };
  },
  view: r => {
    const ask = { mode: r.ask.mode, width: r.ask.width, selected: r.ask.selected ?? undefined };
    const v = guest.blockAdapter.view(blockIn(r.block), ask);
    return { scene: b64(v.scene), sheet: b64(v.sheet), width: v.width, height: v.height, description: v.description };
  },
};

async function init(data) {
  const base = new URL(data.base_url || self.location);
  const grants = new Set(data.grants);
  if (grants.size !== 1 || !grants.has('power:block-adapter')) throw new Error('A block adapter holds exactly power:block-adapter');
  if (data.pack.manifest.requested_scopes.length !== 1 || data.pack.manifest.requested_scopes[0] !== 'power:block-adapter') throw new Error('Grant exceeds reviewed ask');
  await verifyPack(data.pack);
  const component = await fetchBytes(new URL(data.component_file, base));
  const part = data.pack.manifest.parts[0];
  if (data.pack.manifest.parts.length !== 1 || part.role !== 'wasm-component') throw new Error('Not a component');
  verifyPart(part, component);
  if (digest(component) !== data.component_hash) throw new Error('Component identity mismatch');
  // Generated JavaScript is trusted host tooling, not a signed Wasm part: pin the local build's
  // derivation manifest before importing any of it.
  const bindingBytes = await fetchBytes(new URL(data.bindings_file, base));
  if (typeof data.bindings_hash !== 'string' || digest(bindingBytes) !== data.bindings_hash) {
    throw new Error('Component derivation is not pinned by this host');
  }
  const bindings = JSON.parse(new TextDecoder().decode(bindingBytes));
  if (bindings.component_hash !== digest(component)) throw new Error('Bindings belong to another component');
  const files = new Map();
  for (const [name, expected] of Object.entries(bindings.files)) {
    if (!/^library(?:\.core[0-9]*)?\.(js|wasm)$/.test(name)) throw new Error('Unexpected generated file');
    const bytes = await fetchBytes(new URL(`${data.generated_dir}/${name}`, base));
    if (bytes.length !== expected.bytes || digest(bytes) !== expected.hash) throw new Error('Generated binding hash mismatch');
    files.set(name, bytes);
  }
  const moduleURL = URL.createObjectURL(new Blob([files.get('library.js')], { type: 'text/javascript' }));
  const { instantiate } = await import(moduleURL);
  URL.revokeObjectURL(moduleURL);
  guest = await instantiate(path => {
    const bytes = files.get(path);
    if (!bytes) throw new Error('Unlisted core module');
    return WebAssembly.compile(bytes);
  }, {
    ...wasi(message => logs.push(message)),
    // A pure guest's standard library may seed its hash maps; give it a fixed seed, not entropy.
    'wasi:random/insecure-seed': { insecureSeed: () => [0n, 0n] },
    'mere:script/log': { log: message => logs.push(message) },
    'mere:script/caps': { granted: () => [...grants] },
  });
  return { claim: handlers.describe(), component_hash: digest(component), jco: bindings.jco };
}

let tail = Promise.resolve();
self.onmessage = event => {
  tail = tail.then(async () => {
    const { id, command, data } = event.data;
    logs = [];
    try {
      let value;
      if (command === 'init') value = await init(data);
      else if (command === 'call') {
        if (!guest) throw new Error('Block adapter not installed');
        const handler = handlers[data.call];
        if (!handler) throw new Error(`Unknown call ${data.call}`);
        value = handler(data);
      } else if (command === 'stop') guest = undefined;
      else throw new Error('Unknown host command');
      self.postMessage({ id, value, logs });
    } catch (error) {
      // A refusal the component returned is a value for the pane; a trap or host error is too.
      const text = error && typeof error.payload === 'string' ? error.payload : String(error);
      self.postMessage({ id, error: text, logs });
    }
  });
};
