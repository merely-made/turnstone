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

const settle = async () => { for (let turn = 0; turn < 20; turn++) await new Promise(setImmediate); };

const world = {
  now: 0, timers: new Set(), held: new Map(), queued: [], channels: new Set(), posted: [], pages: new Set(),
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
        if (timer.due <= this.now) { timer.due += timer.every; timer.run(); }
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
  const root = { dataset: {}, parentNode: { insertBefore() {} }, nextSibling: null, remove() {} };
  page.elements = elements;
  const context = vm.createContext({
    console, URLSearchParams, AbortController,
    Date: { now: () => world.now },
    setInterval: (run, every) => {
      const timer = { run, every, due: world.now + every };
      world.timers.add(timer);
      return timer;
    },
    clearInterval: (timer) => world.timers.delete(timer),
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
      addEventListener() {}, dispatchEvent() {},
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
  vm.runInContext(source, context);
  page.claim = () => context.api.claimWriter();
  page.unload = () => {
    for (const handler of page.unloaders) handler();
    for (const channel of [...world.channels]) if (channel.owner === owner) channel.close();
    world.release(owner);
    for (const timer of [...world.timers]) if (timer.owner === owner) world.timers.delete(timer);
  };
  // Timers belong to the page that made them.
  const make = context.setInterval;
  context.setInterval = (run, every) => { const timer = make(run, every); timer.owner = owner; return timer; };
  world.pages.add(page);
  return page;
}

function fresh() {
  world.timers.clear(); world.held.clear(); world.queued.length = 0; world.channels.clear();
  world.posted.length = 0; world.reloadRequests = []; world.now = 0;
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

console.log("writer handover smoke ok");
