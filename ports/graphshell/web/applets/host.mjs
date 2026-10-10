// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import { Runtime } from './runtime.mjs';
const hosts = new WeakSet();

async function bytes(url, limit) {
  const response = await fetch(url, { cache: 'no-store' });
  if (!response.ok) throw new Error('Fetch refused: ' + response.status);
  const reader = response.body.getReader();
  const chunks = [];
  let total = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      total += value.length;
      if (total > limit) throw new Error('Host input exceeds size limit');
      chunks.push(value);
    }
  } finally { await reader.cancel(); }
  const result = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { result.set(chunk, offset); offset += chunk.length; }
  return result;
}

// The supplying host provides a disclosed scope and exact applet address.
// The retained Rust view owns review, grants, proposal lowering and rendering.
export async function mountCapsuleApplet(root, gs, supplied) {
  if (hosts.has(root)) throw new Error('This root already has an applet host; use review to replace its scope');
  if (root.dataset.ready !== 'true') throw new Error('Mount Graphshell before its applet host');
  hosts.add(root);
  let source;
  let runtime;
  let disposed = false;
  let pumping = false;
  const transport = { starts: 0, stopped: 0, interrupted: 0, errors: [] };

  async function review(config) {
    if (!/^[0-9a-f]{64}$/.test(config.bindingsHash || '')) throw new Error('A locally trusted derivation address is required');
    const base = new URL(config.baseURL, location.href);
    const [packBytes, component] = await Promise.all([
      bytes(new URL(config.packURL, base), 1024 * 1024),
      bytes(new URL('library.wasm', base), 16 * 1024 * 1024),
    ]);
    // All new material verifies before an existing mount is displaced.
    const generation = gs.review_applet(packBytes, config.packHash, component, JSON.stringify(config.disclosure));
    runtime?.close();
    runtime = undefined;
    source = {
      base, pack: JSON.parse(new TextDecoder().decode(packBytes)),
      component_hash: JSON.parse(gs.applet_receipt()).component_hash,
      entries: config.disclosure.capsules.map(c => c.entry),
      bindings_hash: config.bindingsHash,
    };
    return generation;
  }

  async function apply(generation, result, scope) {
    const opens = JSON.parse(gs.applet_apply_turn(generation, JSON.stringify(result)));
    for (const url of opens) {
      const entry = scope.entries.find(e => e.url === url);
      if (!entry) throw new Error('Address outside supplied scope');
      const kept = await gs.applet_kept_body(generation, url);
      const body = kept || await bytes(new URL('capsules/' + entry.revision + '.gmi', scope.base), entry.bytes);
      gs.applet_open_body(generation, url, body, !!kept);
    }
  }

  async function run(command) {
    const { generation, data } = command;
    const scope = source;
    if (command.command === 'stop') {
      const stopping = runtime;
      runtime = undefined;
      if (stopping) {
        try { await stopping.call('stop', null, 500); }
        finally { stopping.close(); }
      }
      transport.stopped++;
    } else if (command.command === 'start') {
      runtime?.close();
      const active = new Runtime(new URL('worker.mjs', scope.base));
      runtime = active;
      transport.starts++;
      const result = await active.call('init', {
        pack: scope.pack, component_hash: scope.component_hash, entries: scope.entries,
        grants: data, base_url: scope.base.href,
        bindings_hash: scope.bindings_hash,
      }, 15000);
      await apply(generation, result, scope);
    } else if (command.command === 'event') {
      if (!runtime) throw new Error('Applet is not running');
      const result = await runtime.call('event', data);
      await apply(generation, result, scope);
    } else if (command.command === 'grants') {
      if (runtime) await runtime.call('grants', data);
    } else if (command.command === 'keep') {
      await gs.applet_keep(generation);
    } else throw new Error('Unknown host command');
  }

  async function pump() {
    if (disposed || pumping) return;
    pumping = true;
    try {
      const commands = JSON.parse(gs.applet_commands());
      for (const command of commands) {
        try { await run(command); } catch (error) {
          // Failure from displaced work must not stop the replacement.
          if (JSON.parse(gs.applet_receipt()).generation !== command.generation) continue;
          runtime?.close();
          runtime = undefined;
          transport.errors.push(String(error));
          if (String(error).includes('interrupted')) transport.interrupted++;
          gs.applet_failed(command.generation, String(error));
          break;
        }
      }
    } finally {
      pumping = false;
      if (!disposed) setTimeout(pump, 25);
    }
  }

  try { await review(supplied); } catch (error) { hosts.delete(root); throw error; }
  pump();
  return {
    review,
    receipt: () => ({ ...JSON.parse(gs.applet_receipt()), transport: structuredClone(transport) }),
    // Host operations, not new guest imports or grantable envelopes.
    async turn(kind, payload = '') {
      const generation = JSON.parse(gs.applet_receipt()).generation;
      if (!runtime) throw new Error('Applet is not running');
      try {
        const result = await runtime.call('event', { kind, payload });
        await apply(generation, result, source);
      } catch (error) {
        if (JSON.parse(gs.applet_receipt()).generation === generation) {
          runtime?.close(); runtime = undefined;
          if (String(error).includes('interrupted')) transport.interrupted++;
          gs.applet_failed(generation, String(error));
        }
        throw error;
      }
    },
    close() {
      disposed = true;
      hosts.delete(root);
      runtime?.close();
      gs.applet_close();
    },
  };
}
