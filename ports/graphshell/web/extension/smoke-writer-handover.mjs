// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

// Writer and follower roles of the extension page, on a fake clock with a fake
// Web Lock manager and BroadcastChannel shared by every page in the "browser".
// Each page runs the real source in its own context.
const source = fs.readFileSync(new URL("../extension-profile.js", import.meta.url), "utf8")
  .replace(/^export \{[^}]*\};\s*$/m, "globalThis.api = { claimWriter };");
assert.ok(source.includes("globalThis.api"), "the export line was rewritten");

const queueSource = fs.readFileSync(new URL("./follower-queue.js", import.meta.url), "utf8");

const settle = async () => { for (let turn = 0; turn < 20; turn++) await new Promise(setImmediate); };

const world = {
  now: 0, timers: new Set(), held: new Map(), queued: [], channels: new Set(), posted: [], log: [], pages: new Set(),
  grant(name, owner, callback) {
    const lock = { name };
    this.held.set(name, owner);
    // A callback that returns a pending promise keeps the lock until the page goes.
    Promise.resolve(callback(lock)).then(() => this.held.delete(name));
  },
  release(owner) {
    for (const [name, who] of this.held) if (who === owner) this.held.delete(name);
    for (const entry of [...this.queued]) {
      if (this.held.has(entry.name)) continue;
      this.queued.splice(this.queued.indexOf(entry), 1);
      this.grant(entry.name, entry.owner, entry.callback);
    }
  },
  async advance(ms) {
    for (const end = this.now + ms; this.now < end;) {
      this.now = Math.min(end, Math.floor(this.now / 100) * 100 + 100);
      for (const timer of [...this.timers]) {
        if (timer.due <= this.now) {
          if (timer.once) this.timers.delete(timer); else timer.due += timer.every;
          timer.run();
        }
      }
      await settle();
    }
  },
};

function openPage() {
  const owner = {};
  const page = { owner, unloaders: [], role: () => context.graphshellViewRole };
  const element = () => {
    const handlers = {};
    return { hidden: true, textContent: "", handlers,
      addEventListener: (name, handler) => { handlers[name] = handler; } };
  };
  const elements = { "#writer-standby": element(), "#writer-standby-status": element(),
    "#writer-show": element(), "#writer-take": element() };
  const bus = {};
  const root = { dataset: {}, parentNode: { insertBefore() {} }, nextSibling: null, remove() {} };
  page.elements = elements;
  page.context = null;
  const context = vm.createContext({
    console, URLSearchParams, AbortController,
    Date: { now: () => world.now },
    setInterval: (run, every) => {
      const timer = { run, every, due: world.now + every, owner };
      world.timers.add(timer);
      return timer;
    },
    setTimeout: (run, ms) => {
      const timer = { run, every: ms, due: world.now + ms, once: true, owner };
      world.timers.add(timer);
      return timer;
    },
    clearInterval: (timer) => world.timers.delete(timer),
    clearTimeout: (timer) => world.timers.delete(timer),
    Event: class { constructor(type) { this.type = type; } },
    CustomEvent: class { constructor(type, init) { this.type = type; this.detail = init?.detail; } },
    addEventListener: (name, handler) => { if (name === "pagehide") page.unloaders.push(handler); },
    MutationObserver: class { observe() {} },
    location: { search: "", reload: () => { page.reloaded = true; world.reloadRequests.push(page); } },
    chrome: { runtime: { sendMessage: async () => ({ policy: { enabled: false }, visits: [], ids: [] }) } },
    GraphshellCaptureModel: { defaultPolicy: () => ({ enabled: false }), historyFilterFromControls: () => ({}) },
    sessionStorage: { getItem: () => null, removeItem() {} },
    document: {
      body: { dataset: {} },
      querySelector: (selector) => elements[selector]
        ?? (selector === "graphshell-view, graphshell-tree" ? root : null),
      addEventListener: (type, handler) => { (bus[type] ??= []).push(handler); },
      dispatchEvent: (event) => { for (const handler of bus[event.type] ?? []) handler(event); return true; },
    },
    navigator: { locks: {
      query: async () => ({ held: [...world.held.keys()].map((name) => ({ name })) }),
      request(name, options, callback) {
        if (options.ifAvailable) {
          return Promise.resolve(world.held.has(name) ? callback(null) : (world.grant(name, owner, callback), undefined));
        }
        options.signal?.addEventListener?.("abort", () => {});
        return new Promise(() => {
          if (world.held.has(name)) world.queued.push({ name, owner, callback });
          else world.grant(name, owner, callback);
        });
      },
    } },
    BroadcastChannel: class {
      constructor(name) { this.name = name; this.open = true; world.channels.add(this); this.owner = owner; }
      postMessage(data) {
        world.posted.push(data.type);
        world.log.push(data);
        for (const other of world.channels) {
          if (other !== this && other.open && other.name === this.name) {
            setImmediate(() => other.onmessage?.({ data }));
          }
        }
      }
      close() { this.open = false; world.channels.delete(this); }
    },
  });
  context.globalThis = context;
  page.context = context;
  vm.runInContext(queueSource, context);
  vm.runInContext(source, context);
  // A stand-in for the Rust app on this page: it admits each request id once
  // and replays its answer afterwards, or stays silent when told to.
  page.app = { admitted: [], replayed: 0, silent: false, hold: false, held: [], replies: [], ledger: new Map() };
  const dispatch = (type, detail) => context.document.dispatchEvent(new context.CustomEvent(type, { detail }));
  context.document.addEventListener("graphshell-edit-admit", (event) => {
    const request = JSON.parse(event.detail);
    if (page.app.silent) return;
    if (page.app.hold) { page.app.held.push(event); return; }
    let reply = page.app.ledger.get(request.id);
    if (reply) page.app.replayed += 1;
    else {
      reply = { id: request.id, view: request.view, status: "admitted" };
      page.app.ledger.set(request.id, reply);
      page.app.admitted.push(request.id);
    }
    dispatch("graphshell-edit-admitted", JSON.stringify(reply));
  });
  page.app.release = () => {
    page.app.hold = false;
    for (const event of page.app.held.splice(0)) context.document.dispatchEvent(event);
  };
  context.document.addEventListener("graphshell-edit-reply", (event) => page.app.replies.push(JSON.parse(event.detail)));
  page.queue = (id) => dispatch("graphshell-edit-submit", JSON.stringify({ id, view: context.graphshellViewId, base: { session: "s", epoch: 1 }, edit: { kind: "add_node", address: `https://example.org/${id}`, title: "" } }));
  page.rawEdit = (id) => {
    const channel = new context.BroadcastChannel("graphshell.local-profile.views.v1");
    channel.postMessage({ type: "edit", request: { id, view: "raw" } });
    channel.close();
  };
  page.claim = () => context.api.claimWriter();
  page.unload = () => {
    for (const handler of page.unloaders) handler();
    for (const channel of [...world.channels]) if (channel.owner === owner) channel.close();
    world.release(owner);
    for (const timer of [...world.timers]) if (timer.owner === owner) world.timers.delete(timer);
  };
  world.pages.add(page);
  return page;
}

function fresh() {
  world.timers.clear(); world.held.clear(); world.queued.length = 0; world.channels.clear();
  world.posted.length = 0; world.log.length = 0; world.reloadRequests = []; world.now = 0;
}

// The writer reloads itself: a polling follower must not take the lock first.
fresh();
let writer = openPage();
assert.equal(await writer.claim(), "writer");
let follower = openPage();
assert.equal(await follower.claim(), "follower");
writer.unload();
await settle();
assert.ok(world.posted.includes("reloading"), "an unloading writer announces itself");
await world.advance(2000); // the follower polls into the vacancy
assert.equal(follower.role(), "follower", "the follower holds off during the grace");
const reloaded = openPage();
assert.equal(await reloaded.claim(), "writer", "the reloaded page claims the lock first");
await world.advance(10000);
assert.equal(follower.role(), "follower", "the follower stays a follower");
assert.equal(reloaded.role(), "writer");

// The writer closes and nothing claims: the follower takes over after the grace.
fresh();
writer = openPage();
await writer.claim();
follower = openPage();
await follower.claim();
writer.unload();
await settle();
await world.advance(2000);
assert.equal(follower.role(), "follower", "no takeover inside the grace");
await world.advance(2100);
assert.equal(follower.role(), "writer", "the follower takes over once the grace has passed");

// "Edit here": a deliberate handover swaps the roles and announces no reload.
fresh();
writer = openPage();
await writer.claim();
follower = openPage();
await follower.claim();
follower.elements["#writer-take"].handlers.click();
await settle();
assert.ok(writer.reloaded, "the writer reloads to hand over");
writer.unload();
await settle();
assert.ok(!world.posted.includes("reloading"), "a handover is not announced as a reload");
assert.equal(follower.role(), "writer", "the asking view is granted the lock");
const back = openPage();
assert.equal(await back.claim(), "follower", "the old writer returns following");

const edits = (id) => world.log.filter((m) => m.type === "edit" && m.request?.id === id).length;

// A follower's edit reaches the writer, is admitted once and answered.
fresh();
writer = openPage();
await writer.claim();
follower = openPage();
await follower.claim();
assert.ok(follower.context.graphshellViewId, "a follower names its view");
follower.queue("a");
await settle();
assert.deepEqual(writer.app.admitted, ["a"], "the writer admitted the edit");
assert.equal(follower.app.replies.length, 1);
assert.equal(follower.app.replies[0].status, "admitted", "the follower is told it was admitted");
await world.advance(60000);
assert.equal(edits("a"), 1, "an answered edit is never sent again");
assert.equal(follower.app.replies.length, 1);

// Duplicate delivery: the same id arriving again while it is stored, and again
// after it was answered, is admitted once.
writer.app.hold = true;
follower.queue("b");
await settle();
follower.rawEdit("b");
await settle();
assert.equal(writer.app.held.length, 1, "handed to the app once while it is storing it");
writer.app.release();
await settle();
follower.rawEdit("b");
await settle();
follower.rawEdit("b");
await settle();
assert.equal(writer.app.admitted.filter((id) => id === "b").length, 1, "admitted once");
assert.ok(writer.app.replayed >= 1, "a repeat is answered from the record, not applied");
assert.equal(follower.app.replies.filter((r) => r.id === "b").length, 1, "the follower is told once");

// A writer that hears the edit and never answers: one resend, then a refusal.
fresh();
writer = openPage();
await writer.claim();
writer.app.silent = true;
follower = openPage();
await follower.claim();
follower.queue("c");
await settle();
await world.advance(14000);
assert.equal(follower.app.replies.length, 0, "still waiting inside the reply window");
await world.advance(30000);
assert.equal(follower.app.replies.length, 1);
assert.equal(follower.app.replies[0].status, "refused");
assert.match(follower.app.replies[0].reason, /No answer/);
assert.equal(edits("c"), 2, "sent once and once more");
await world.advance(60000);
assert.equal(edits("c"), 2);

// The writer goes mid-request: the follower that takes over is sent the edit
// again, once, and answers it. The first follower hears the answer.
fresh();
writer = openPage();
await writer.claim();
writer.app.silent = true;
// The view that waits longest (opened first) takes the lock; the other queued the edit.
const heir = openPage();
await heir.claim();
follower = openPage();
await follower.claim();
follower.queue("d");
await settle();
assert.equal(edits("d"), 1);
writer.unload();
await settle();
await world.advance(10000);
assert.equal(heir.role(), "writer", "a follower took the lock");
assert.equal(follower.role(), "follower");
await world.advance(1000);
assert.equal(heir.app.admitted.filter((id) => id === "d").length, 1, "the new writer admitted it once");
assert.equal(follower.app.replies.filter((r) => r.id === "d").length, 1, "the asking view hears the answer");
assert.equal(edits("d"), 2, "sent once more to the new writer, not repeatedly");
await world.advance(60000);
assert.equal(edits("d"), 2);
assert.ok(follower.app.replies.every((r) => r.status === "admitted"));

// A writer admitting an edit does not hand over until it has answered.
fresh();
writer = openPage();
await writer.claim();
writer.app.silent = true;
follower = openPage();
await follower.claim();
follower.queue("e");
await settle();
follower.elements["#writer-take"].handlers.click();
await settle();
assert.ok(!writer.reloaded, "no reload while an edit is being stored");
assert.ok(world.posted.includes("busy"), "the asking view is told it is busy");

console.log("writer handover smoke ok");
