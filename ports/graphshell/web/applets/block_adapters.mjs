// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// The supplying host for guest block adapters: it discloses the packs it was given, shows the
// person a review, and loads a component into a Worker only after they approve.
//
// Flow (mirrors `host.mjs` for applets, and `CapsuleMount::review` on the Rust side):
//   1. The pane's "Review block adapters" sets a request; this host's pump sees it and opens the
//      review panel, which lists the disclosed packs.
//   2. "Review" fetches a pack and its component and hands the exact bytes to
//      `block_adapter_review`, which verifies the signature, the component's address and size,
//      and that the pack asks for `power:block-adapter` alone. Nothing runs; the panel shows
//      the pack's name, author key, ask, component address and size, and the Moot, collection
//      and revision that disclosed it (`build_block_adapters.mjs --moot` writes a real one;
//      `--pack` writes a local-test disclosure that says so).
//   3. "Approve block adapter" grants that one power (`block_adapter_approve`), starts the
//      Worker (`block_worker.mjs`, which verifies everything again), asks the component to
//      describe itself, and registers the class it claims (`block_adapter_install`).
//   4. From then on the pane's calls cross here: the pump takes the requests, the Worker
//      answers, and the replies go back. A Worker that overruns its deadline is terminated and
//      its class is removed, so its blocks return to plain text.
// Declining, or closing the panel, loads nothing.

import { Runtime } from './runtime.mjs';
import { fetchBytes } from './verify.mjs';

const hosts = new WeakSet();
const TURN_MS = 5000;

const el = (tag, props = {}, ...kids) => {
  const node = Object.assign(document.createElement(tag), props);
  for (const kid of kids) node.append(kid);
  return node;
};

export async function mountBlockAdapters(root, gs, supplied) {
  for (const name of ['block_adapter_review', 'block_adapter_approve', 'block_adapter_install',
    'block_adapter_take_requests', 'block_adapter_reply', 'block_adapter_remove', 'block_adapter_review_requested', 'block_pane_open']) {
    if (typeof gs[name] !== 'function') throw new Error(`This Graphshell build has no ${name}; build the product with block adapters`);
  }
  if (hosts.has(root)) throw new Error('This root already has a block adapter host');
  if (root.dataset.ready !== 'true') throw new Error('Mount Graphshell before its block adapter host');
  hosts.add(root);
  const base = new URL(supplied.baseURL, location.href);
  const runtimes = new Map(); // class -> Runtime
  const transport = { reviews: 0, approvals: 0, installs: 0, calls: 0, refusals: 0, interrupted: 0, errors: [] };
  let reviewed; // { entry, pack, component_hash, receipt }
  let disposed = false;

  // --- the review panel -----------------------------------------------------------------
  const status = el('p', { role: 'status', className: 'block-adapter-status' });
  const list = el('div', { className: 'block-adapter-list' });
  const detail = el('div', { className: 'block-adapter-detail' });
  const panel = el('section', { id: 'block-adapter-review', hidden: true },
    el('h2', { textContent: 'Block adapters' }),
    el('p', { textContent: 'A block adapter is a signed component that edits and draws one kind of block. It runs in a sandbox with no files, network or clock. Review it, then approve it or leave it unloaded.' }),
    list, detail, status,
    el('button', { type: 'button', textContent: 'Close review', className: 'block-adapter-close', onclick: () => { panel.hidden = true; root.dataset.blockAdapterReview = 'closed'; } }));
  panel.setAttribute('role', 'dialog');
  panel.setAttribute('aria-label', 'Block adapters');
  Object.assign(panel.style, { position: 'fixed', right: '12px', top: '12px', maxWidth: '420px', maxHeight: '90vh', overflow: 'auto', overflowWrap: 'anywhere', zIndex: 20, padding: '12px', background: '#16191d', color: '#e8e6e1', border: '1px solid #6d7783', font: '14px sans-serif' });
  document.body.append(panel);
  const say = text => { status.textContent = text; };
  const setState = state => { root.dataset.blockAdapterState = state; };
  setState('idle');

  function renderList() {
    list.replaceChildren(...supplied.adapters.map(entry => {
      const loaded = [...runtimes.keys()].length > 0 && entry.loaded;
      return el('div', {},
        el('p', { textContent: `${entry.name}${entry.loaded ? ' · loaded' : ''}` }),
        el('button', { type: 'button', textContent: `Review ${entry.name}`, disabled: !!entry.loaded, className: 'block-adapter-review-button', onclick: () => review(entry).catch(fail) }));
    }));
  }

  function fail(error) {
    transport.errors.push(String(error));
    say(`Refused: ${error && error.message ? error.message : error}`);
    setState('refused');
  }

  async function review(entry) {
    setState('reviewing');
    say('Fetching and verifying the pack…');
    const [packBytes, component] = await Promise.all([
      fetchBytes(new URL(entry.pack_file, base)),
      fetchBytes(new URL(entry.component_file, base)),
    ]);
    // New material verifies completely before it replaces a review in progress.
    const receipt = JSON.parse(gs.block_adapter_review(packBytes, entry.pack_hash, component, JSON.stringify(entry.disclosure)));
    transport.reviews++;
    reviewed = { entry, pack: JSON.parse(new TextDecoder().decode(packBytes)), component_hash: receipt.component_hash, receipt };
    detail.replaceChildren(
      el('p', { textContent: `${receipt.name} ${receipt.version}`, className: 'block-adapter-name' }),
      el('p', { textContent: `Author key: ${receipt.author}`, className: 'block-adapter-author' }),
      el('p', { textContent: `Asks for: ${receipt.asks.join(', ')}`, className: 'block-adapter-asks' }),
      el('p', { textContent: `Component: ${receipt.component_bytes} bytes · ${receipt.component_hash}`, className: 'block-adapter-component' }),
      el('p', { textContent: `Disclosed by moot ${receipt.moot}, collection ${receipt.collection}, revision ${receipt.revision}`, className: 'block-adapter-disclosure' }),
      el('p', { textContent: 'Signature, component address and size verified. Nothing has run.' }),
      el('button', { type: 'button', textContent: 'Approve block adapter', className: 'block-adapter-approve', onclick: () => approve().catch(fail) }),
      el('button', { type: 'button', textContent: 'Decline', className: 'block-adapter-decline', onclick: () => { reviewed = undefined; detail.replaceChildren(); say('Declined; nothing was loaded.'); setState('declined'); } }));
    say('Reviewed. Approve to grant power:block-adapter and load it, or decline.');
    setState('reviewed');
  }

  async function approve() {
    if (!reviewed) throw new Error('Review a block adapter first');
    const { entry, pack, component_hash } = reviewed;
    gs.block_adapter_approve();
    transport.approvals++;
    say('Approved. Starting the sandbox…');
    const runtime = new Runtime(new URL('block_worker.mjs', base));
    try {
      const result = await runtime.call('init', {
        pack, component_hash, grants: ['power:block-adapter'], base_url: base.href,
        component_file: entry.component_file, bindings_file: entry.bindings_file,
        bindings_hash: entry.bindings_hash, generated_dir: entry.generated_dir,
      }, 20000);
      const klass = gs.block_adapter_install(JSON.stringify(result.value.claim));
      runtimes.set(klass, runtime);
      entry.loaded = true;
      transport.installs++;
      detail.replaceChildren();
      renderList();
      say(`Loaded: blocks of class “${klass}” now use this adapter.`);
      setState('loaded');
      root.dataset.blockAdapterClasses = [...runtimes.keys()].join(',');
    } catch (error) {
      runtime.close();
      throw error;
    }
  }

  function drop(klass, why) {
    const runtime = runtimes.get(klass);
    runtimes.delete(klass);
    runtime?.close();
    gs.block_adapter_remove(klass);
    for (const entry of supplied.adapters) entry.loaded = false;
    renderList();
    say(`The block adapter for “${klass}” was removed: ${why}. Its blocks are plain text again.`);
    setState('removed');
    root.dataset.blockAdapterClasses = [...runtimes.keys()].join(',');
  }

  // --- the pump ---------------------------------------------------------------------------
  const inflight = new Set();
  async function serve({ class: klass, id, request }) {
    const runtime = runtimes.get(klass);
    if (!runtime) { gs.block_adapter_reply(klass, id, JSON.stringify({ err: 'The block adapter is not running' })); return; }
    transport.calls++;
    let reply;
    try {
      const result = await runtime.call('call', JSON.parse(request), TURN_MS);
      reply = { ok: result.value };
    } catch (error) {
      const text = String(error && error.message ? error.message : error);
      if (text.includes('interrupted') || text.includes('stopped')) {
        transport.interrupted++;
        drop(klass, text);
        return;
      }
      transport.refusals++;
      reply = { err: text };
    }
    if (runtimes.has(klass)) gs.block_adapter_reply(klass, id, JSON.stringify(reply));
  }

  function pump() {
    if (disposed) return;
    try {
      if (gs.block_adapter_review_requested()) {
        panel.hidden = false;
        renderList();
        say('Choose a disclosed block adapter to review.');
        setState('review-open');
        root.dataset.blockAdapterReview = 'open';
      }
      for (const request of JSON.parse(gs.block_adapter_take_requests())) {
        const work = serve(request).finally(() => inflight.delete(work));
        inflight.add(work);
      }
    } catch (error) {
      transport.errors.push(String(error));
    }
    setTimeout(pump, 25);
  }
  // A launcher for the document editor that does not depend on the page's toolbar.
  const launcher = el('button', { type: 'button', textContent: 'Open block pane', className: 'block-pane-open', onclick: () => gs.block_pane_open(true) });
  Object.assign(launcher.style, { position: 'fixed', right: '12px', bottom: '12px', zIndex: 19, padding: '6px 10px', background: '#22262c', color: '#e8e6e1', border: '1px solid #6d7783', font: '14px sans-serif' });
  document.body.append(launcher);
  renderList();
  pump();
  return {
    openPane: () => gs.block_pane_open(true),
    receipt: () => ({ ...(typeof gs.block_adapter_receipt === 'function' ? JSON.parse(gs.block_adapter_receipt()) : {}), transport: structuredClone(transport) }),
    // Test hooks that do what the buttons do.
    review: name => review(supplied.adapters.find(a => a.name === name) ?? supplied.adapters[0]),
    approve,
    close() {
      disposed = true;
      hosts.delete(root);
      for (const runtime of runtimes.values()) runtime.close();
      runtimes.clear();
      panel.remove();
      launcher.remove();
    },
  };
}
