// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import { digest, verifyPack, verifyPart, fetchBytes } from './verify.mjs';
import { wasi } from './wasi.mjs';
let guest;
let scope;
let grants = new Set();
let actions = [];
let logs = [];
let refusals = [];
function refuse(tag, val) { refusals.push({ tag, val }); throw { tag, val }; }
function emit({ name, payload }) {
  const power = name === 'capsule-library-view' ? 'power:capsule-view' : name === 'open-address' ? 'power:navigate' : undefined;
  if (name === 'confirm-install-participant') refuse('denied', 'host-only');
  if (!power) refuse('unknown', name);
  if (!grants.has(power)) refuse('denied', power);
  let parsed;
  try { parsed = JSON.parse(payload); } catch { refuse('malformed', 'Invalid JSON'); }
  if (name === 'capsule-library-view') {
    if (!Array.isArray(parsed)) refuse('malformed', 'Expected entries');
    const seen = new Set();
    for (const entry of parsed) {
      const original = scope.entries.find(e => e.revision === entry.revision);
      if (!original || seen.has(entry.revision) || Object.keys(original).some(k => original[k] !== entry[k])) refuse('denied', 'outside disclosed catalogue');
      seen.add(entry.revision);
    }
  } else {
    if (typeof parsed?.url !== 'string') refuse('malformed', 'Missing url');
    if (!scope.entries.some(e => e.url === parsed.url)) refuse('denied', 'outside disclosed catalogue');
  }
  actions.push({ name, payload: parsed });
}

async function init(data) {
  scope = data;
  const base = data.base_url || self.location;
  grants = new Set(data.grants);
  if ([...grants].some(cap => !scope.pack.manifest.requested_scopes.includes(cap))) throw new Error('Grant exceeds reviewed ask');
  await verifyPack(scope.pack);
  const component = await fetchBytes(new URL('library.wasm', base));
  const part = scope.pack.manifest.parts[0];
  if (part.role !== 'wasm-component') throw new Error('Not a component');
  verifyPart(part, component);
  if (digest(component) !== scope.component_hash) throw new Error('Component identity mismatch');
  // Generated JavaScript is trusted host tooling, not a signed Wasm part.
  // Pin the local build's derivation manifest before importing any of it.
  const bindingBytes = await fetchBytes(new URL('bindings.json', base));
  if (typeof scope.bindings_hash !== 'string' || digest(bindingBytes) !== scope.bindings_hash) {
    throw new Error('Component derivation is not pinned by this host');
  }
  const bindings = JSON.parse(new TextDecoder().decode(bindingBytes));
  if (bindings.component_hash !== digest(component)) throw new Error('Bindings belong to another component');
  const files = new Map();
  for (const [name, expected] of Object.entries(bindings.files)) {
    if (!/^library(?:\.core[0-9]*)?\.(js|wasm)$/.test(name)) throw new Error('Unexpected generated file');
    const bytes = await fetchBytes(new URL(`generated/${name}`, base));
    if (bytes.length !== expected.bytes || digest(bytes) !== expected.hash) throw new Error('Generated binding hash mismatch');
    files.set(name, bytes);
  }
  // Load exact checked JS bytes and compile exact checked core modules. The
  // trusted jco adapter is generated locally from the signed component.
  const moduleURL = URL.createObjectURL(new Blob([files.get('library.js')], { type: 'text/javascript' }));
  const { instantiate } = await import(moduleURL);
  URL.revokeObjectURL(moduleURL);
  guest = await instantiate(path => {
    const bytes = files.get(path);
    if (!bytes) throw new Error('Unlisted core module');
    return WebAssembly.compile(bytes);
  }, {
    ...wasi(message => logs.push(message)),
    'mere:script/log': { log: message => logs.push(message) },
    'mere:script/caps': { granted: () => [...grants] },
    'mere:script/actions': { emit },
  });
  guest.activate();
  return { component_hash: digest(component), jco: bindings.jco };
}

let tail = Promise.resolve();
self.onmessage = event => {
  tail = tail.then(async () => {
    const { id, command, data } = event.data;
    actions = []; logs = []; refusals = [];
    try {
      let value;
      if (command === 'init') value = await init(data);
      else if (command === 'grants') {
        // Revocation can only narrow this instance's grant.
        if (data.some(cap => !grants.has(cap))) throw new Error('Grant widening requires a new review');
        grants = new Set(data);
      } else if (command === 'event') {
        if (!guest) throw new Error('Component not installed');
        guest.onEvent(data.kind, data.payload);
      } else if (command === 'stop') guest?.deactivate();
      else throw new Error('Unknown host command');
      self.postMessage({ id, value, actions, logs, refusals });
    } catch (error) {
      self.postMessage({ id, error: String(error), actions: [], logs, refusals });
    }
  });
};
