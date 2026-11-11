// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// "Keep page text": the worker's queue and drain of extracted text, in the
// style of smoke-capture-background.mjs, and the in-page extractor against a
// small fake document. The writer view's recording into PageTextStore is the
// Rust side (`kept_text_tests`); the headed run joins them.

import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const event = () => ({ listeners: [], addListener(listener) { this.listeners.push(listener); } });
const runtimeMessage = event();
const tabUpdated = event();
const permissionRemoved = event();
const stored = {};
const clock = { now: 1_000_000 };
const announced = [];
const injections = [];
let granted = false;
let injectResult = null;
let injectFailure = null;

const chrome = {
  action: { onClicked: event() },
  tabs: { create: async () => {}, onUpdated: tabUpdated },
  runtime: {
    id: "graphshell",
    getURL: (path) => `chrome-extension://graphshell/${path}`,
    onMessage: runtimeMessage,
    async sendMessage(message) { announced.push(message); throw new Error("Receiving end does not exist."); },
  },
  storage: {
    local: {
      async get(keys) { return Object.fromEntries(keys.filter((key) => key in stored).map((key) => [key, stored[key]])); },
      async set(values) { Object.assign(stored, structuredClone(values)); },
    },
  },
  permissions: { contains: async () => granted, onAdded: event(), onRemoved: permissionRemoved },
  scripting: {
    async executeScript(details) {
      injections.push(details);
      if (injectFailure) throw new Error(injectFailure);
      return [{ result: typeof injectResult === "function" ? injectResult(details) : injectResult }];
    },
  },
};

const context = vm.createContext({
  chrome, console, URL, TextEncoder, structuredClone,
  Date: { now: () => clock.now },
  crypto: { randomUUID: () => "fixture-scope" },
  globalThis: null,
  importScripts() {},
});
context.globalThis = context;
for (const file of ["capture-model.js", "background.js"]) {
  vm.runInContext(fs.readFileSync(new URL(`./${file}`, import.meta.url), "utf8"), context);
}
const model = context.GraphshellCaptureModel;

const PAGE = { id: "graphshell", url: "chrome-extension://graphshell/graph.html" };
function send(message) {
  return new Promise((resolve) => { runtimeMessage.listeners[0](message, PAGE, resolve); })
    .then((response) => { if (response?.error) throw new Error(response.error); return response; });
}
async function loaded(tab, url, text, extra = {}) {
  injectResult = text === null ? null : { url, title: "t", text };
  clock.now += 10;
  for (const listener of tabUpdated.listeners) {
    listener(tab, { status: "complete" }, { id: tab, url, incognito: false, ...extra });
  }
  await new Promise((resolve) => setTimeout(resolve, 20));
}
const status = () => send({ type: "keep.status" });

// Off by default: a loaded page is not read.
let current = await status();
assert.deepEqual({ ...current.policy }, { enabled: false, max_bytes: 128 * 1024 });
await loaded(1, "https://garden.example/compost", "Turn the pile weekly.");
assert.equal(injections.length, 0, "nothing is injected while the control is off");
assert.equal((await status()).queued, 0);

// Enabling needs the optional access; refusal leaves the control off.
await assert.rejects(send({ type: "keep.configure", policy: { enabled: true } }), /not been granted/);
assert.equal((await status()).policy.enabled, false);
granted = true;
await send({ type: "keep.configure", policy: { enabled: true, max_bytes: 64 } });
current = await status();
assert.equal(current.policy.enabled, true);
assert.equal(current.policy.max_bytes, 1024, "an absurd bound is raised to the floor");
await send({ type: "keep.configure", policy: { enabled: true, max_bytes: 4096 } });

// It is a separate control: live remembering is still off.
assert.equal((await send({ type: "capture.status" })).policy.enabled, false);

// A loaded page is read in its tab and queued, fragment stripped.
await loaded(7, "https://garden.example/compost#top", "Turn the pile weekly.  Browns\n balance greens.");
assert.equal(injections.length, 1);
assert.equal(injections[0].target.tabId, 7);
assert.equal(typeof injections[0].func, "function");
assert.deepEqual([...injections[0].args], [4096]);
current = await status();
assert.equal(current.queued, 1);
assert.ok(announced.some((message) => message.type === "capture.available"), "an open writer view is told");
let pulled = await send({ type: "keep.pull" });
assert.equal(pulled.pages.length, 1);
assert.deepEqual({ ...pulled.pages[0] }, {
  url: "https://garden.example/compost", text: "Turn the pile weekly. Browns balance greens.",
  at_ms: clock.now, private: false });
assert.equal(pulled.policy.enabled, true);

// Skips: private window, excluded origin, other scheme, a non-HTML document.
await send({ type: "capture.configure", policy: { excluded_origins: ["https://private.example"] } });
const before = injections.length;
await loaded(2, "https://garden.example/p", "private body", { incognito: true });
await loaded(3, "https://private.example/x", "excluded body");
await loaded(4, "file:///etc/hosts", "local file");
await loaded(5, "chrome://settings", "internal");
assert.equal(injections.length, before, "skipped pages are not even read");
await loaded(6, "https://garden.example/doc.pdf", null);
current = await status();
assert.equal(current.queued, 1);
assert.equal(current.state.skipped, 5);
pulled = await send({ type: "keep.pull" });
assert.deepEqual([...pulled.policy.excluded_origins], ["https://private.example"]);

// A page the browser keeps from extensions is counted and explained.
injectFailure = "Cannot access contents of the page.";
await loaded(8, "https://chromewebstore.google.com/", "x");
injectFailure = null;
current = await status();
assert.equal(current.state.failed, 1);
assert.match(current.state.last_error, /Cannot access contents/);

// Size is bounded in bytes without splitting a character; the cut is counted.
await loaded(9, "https://garden.example/long", "é".repeat(5000));
pulled = await send({ type: "keep.pull" });
const long = pulled.pages.find((page) => page.url.endsWith("/long"));
assert.ok(new TextEncoder().encode(long.text).length <= 4096 && long.text.length === 2048, long.text.length);
assert.equal((await status()).state.truncated, 1);
assert.deepEqual({ ...model.boundText("a😀b", 5) }, { text: "a😀", truncated: true });
assert.deepEqual({ ...model.boundText("a😀b", 4) }, { text: "a", truncated: true }, "never half a pair");

// Loading a page again replaces its pending text (one per address).
await loaded(7, "https://garden.example/compost", "Second reading.");
pulled = await send({ type: "keep.pull" });
const compost = pulled.pages.filter((page) => page.url.endsWith("/compost"));
assert.deepEqual(Array.from(compost, (page) => page.text), ["Second reading."]);

// Drain: ack removes only what was delivered; an older ack cannot remove a newer text.
const stale = pulled.keys.find((key) => key.startsWith("https://garden.example/compost"));
await loaded(7, "https://garden.example/compost", "Third reading.");
await send({ type: "keep.ack", keys: [stale], kept: 1, unchanged: 0, refused: 0 });
pulled = await send({ type: "keep.pull" });
assert.ok(pulled.pages.some((page) => page.text === "Third reading."));
await send({ type: "keep.ack", keys: pulled.keys, kept: 2, unchanged: 1, refused: 1 });
current = await status();
assert.equal(current.queued, 0);
assert.deepEqual([current.state.kept, current.state.unchanged, current.state.refused], [3, 1, 1]);
await assert.rejects(send({ type: "keep.ack", keys: [1] }), /Invalid keep acknowledgement/);

// A writer that cannot record (a full store) leaves the pages queued and says so.
await loaded(10, "https://garden.example/two", "Another page.");
await send({ type: "keep.fail", error: "QuotaExceededError" });
current = await status();
assert.equal(current.queued, 1);
assert.equal(current.state.last_error, "QuotaExceededError");

// The queue is bounded by count; what falls off is counted, not silent.
for (let n = 0; n < 60; n += 1) await loaded(20 + n, `https://garden.example/n${n}`, `page ${n}`);
current = await status();
assert.equal(current.queued, 48);
assert.ok(current.state.omitted_queue >= 13, current.state.omitted_queue);
const delivery = (await send({ type: "keep.pull" }));
assert.ok(delivery.pages.length <= 8, "a delivery is a few pages, not the queue");

// Stopping discards waiting text and nothing more is read.
const reads = injections.length;
await send({ type: "keep.configure", policy: { enabled: false } });
assert.equal((await status()).queued, 0);
await loaded(99, "https://garden.example/after", "after stop");
assert.equal(injections.length, reads);
assert.equal((await send({ type: "keep.pull" })).pages.length, 0);

// Removing the browser permission turns it off and drops waiting text.
await send({ type: "keep.configure", policy: { enabled: true } });
await loaded(100, "https://garden.example/w", "waiting");
assert.equal((await status()).queued, 1);
granted = false;
permissionRemoved.listeners.forEach((listener) => listener({ permissions: ["scripting"], origins: ["<all_urls>"] }));
await new Promise((resolve) => setTimeout(resolve, 20));
current = await status();
assert.deepEqual([current.policy.enabled, current.queued, current.permission], [false, 0, false]);

// ---- The writer view's side: hand the queue to the app, acknowledge on its word ----
globalThis.chrome = chrome;
globalThis.document = { querySelector: () => null };
const { finishKeep, prepareKeep, pullKeep } = await import("../extension-keep.js");
granted = true;
await send({ type: "keep.configure", policy: { enabled: true, max_bytes: 4096 } });
await loaded(300, "https://garden.example/drain-a", "First kept page about worms.");
await loaded(301, "https://garden.example/drain-b", "Second kept page about leaf mould.");

// Opening: the pages queued while no view was open go to the app as a global.
const opening = await prepareKeep(send);
const handed = JSON.parse(globalThis.graphshellInitialKeptJson);
assert.equal(handed.policy.enabled, true);
assert.deepEqual(handed.pages.map((page) => page.url).sort(),
  ["https://garden.example/drain-a", "https://garden.example/drain-b"]);
assert.equal(opening.keys.length, 2);
// Nothing is acknowledged until the app reports; a quota failure keeps the pages and says why.
assert.equal((await status()).queued, 2);
const failed = await finishKeep(opening.keys, { keepError: "QuotaExceededError" }, "keep");
assert.equal(failed.failed, true);
current = await status();
assert.equal(current.queued, 2, "failed pages stay queued");
assert.equal(current.state.last_error, "QuotaExceededError");
// The app recorded them: counts come from what it reported, and the queue empties.
const landed = await finishKeep(opening.keys, { keepKept: "2", keepUnchanged: "0", keepRefused: "0", keepError: "" }, "keep");
assert.equal(landed.failed, false);
current = await status();
assert.equal(current.queued, 0);
assert.equal(current.state.last_error, null);

// Live: a page loaded while the view is open is pulled as a delivery and
// acknowledged under the batch counters.
assert.equal(await pullKeep(), null, "nothing waiting");
await loaded(302, "https://garden.example/live", "Live page about vermicompost.");
const live = await pullKeep();
assert.equal(live.pages.length, 1);
assert.equal(live.policy.enabled, true);
assert.equal((await finishKeep(live.keys, { keepBatchKept: "0", keepBatchUnchanged: "1", keepBatchRefused: "0", keepBatchError: "" }, "keepBatch")).failed, false);
current = await status();
assert.deepEqual([current.queued, current.state.kept, current.state.unchanged], [0, 2, 1]);
assert.deepEqual(await finishKeep([], {}, "keepBatch"), { failed: false }, "an empty delivery is not an acknowledgement");

// ---- The in-page extractor, against a small fake document -----------------
const el = (tagName, children = [], attributes = {}) => {
  const node = { tagName, children, parentElement: null, hidden: false, ...attributes,
    getAttribute: (name) => attributes[name] ?? null,
    get textContent() { return children.map((child) => child.textContent ?? child.nodeValue).join(""); } };
  children.forEach((child) => { child.parentElement = node; child.parentNode = node; });
  return node;
};
const text = (nodeValue) => ({ nodeValue, nodeType: 3 });
const prose = "Composting needs air, water, browns and greens. ".repeat(6);
const article = el("ARTICLE", [el("H1", [text("Compost")]), el("P", [text(prose)]),
  el("P", [text("Hidden aside text")], { "aria-hidden": "true" }),
  el("SCRIPT", [text("alert(1)")]), el("NAV", [text("Home About")]),
  el("P", [text("Turn the pile weekly.")])]);
const body = el("BODY", [el("HEADER", [text("Site banner")]), el("MAIN", [article]), el("FOOTER", [text("Copyright")])]);
const walk = (root) => { const out = []; (function visit(n) { if (n.nodeType === 3) out.push(n); else n.children?.forEach(visit); })(root); return out; };
const extractContext = vm.createContext({
  NodeFilter: { SHOW_TEXT: 4 },
  location: { href: "https://garden.example/compost" },
  document: {
    contentType: "text/html", title: "Compost", body,
    querySelector: (selector) => (selector === "main" ? body.children[1] : selector === "article" ? article : null),
    createTreeWalker: (root) => { const nodes = walk(root); let index = -1;
      return { nextNode: () => { index += 1; return Boolean(nodes[index]); },
        get currentNode() { return nodes[index]; } }; },
  },
});
const extractor = vm.runInContext(`(${model.extractPageText.toString()})`, extractContext);
const read = extractor(2000);
assert.equal(read.url, "https://garden.example/compost");
assert.ok(read.text.startsWith("Compost Composting needs air"), read.text.slice(0, 60));
assert.ok(read.text.includes("Turn the pile weekly."));
for (const absent of ["alert(1)", "Home About", "Hidden aside", "Site banner", "Copyright"]) {
  assert.ok(!read.text.includes(absent), `${absent} must not be read`);
}
assert.ok(extractor(120).text.length <= 120, "extraction stops at its bound");
extractContext.document.contentType = "application/pdf";
assert.equal(extractor(2000), null);

console.log("keep page text smoke ok");
