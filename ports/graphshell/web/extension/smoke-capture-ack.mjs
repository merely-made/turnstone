// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import "./capture-model.js";

// The page side of the acknowledgement: the extension page pulls the queue,
// leaves it queued until the host announces `graphshell-capture-complete`, then
// acknowledges the pulled visits and reads the counters from the Graphshell root.
const listeners = [];
const stored = {};
const chrome = {
  action: { onClicked: { addListener() {} } },
  tabs: { create: async () => {} },
  runtime: {
    id: "graphshell",
    getURL: (path) => `chrome-extension://graphshell/${path}`,
    onMessage: { addListener: (listener) => listeners.push(listener) },
    sendMessage: (message) => new Promise((resolve) => listeners[0](message, {}, resolve)),
  },
  storage: { local: {
    async get(keys) { return Object.fromEntries(keys.filter((key) => key in stored).map((key) => [key, stored[key]])); },
    async set(values) { Object.assign(stored, values); },
  } },
  permissions: { contains: async () => true, onAdded: { addListener() {} }, onRemoved: { addListener() {} } },
  history: {},
};
const context = vm.createContext({ chrome, console, URL, crypto: { randomUUID: () => "ack-scope" } });
context.globalThis = context;
for (const file of ["capture-model.js", "background.js"]) {
  vm.runInContext(fs.readFileSync(new URL(`./${file}`, import.meta.url), "utf8"), context);
}

const root = { dataset: {} };
const status = { textContent: "", dataset: {} };
const documentListeners = new Map();
globalThis.chrome = chrome;
globalThis.document = {
  querySelector: (selector) => selector === "#capture-status" ? status
    : selector === "graphshell-view, graphshell-tree" ? root : null,
  addEventListener: (name, listener, options) => documentListeners.set(name, { listener, once: options?.once }),
  dispatch(name) {
    const entry = documentListeners.get(name);
    if (entry?.once) documentListeners.delete(name);
    return entry?.listener();
  },
};
const settle = async () => { for (let turn = 0; turn < 20; turn++) await new Promise(setImmediate); };
const send = (message) => chrome.runtime.sendMessage(message);

const visit = (id, at) => ({
  source: "firefox", visit_id: id, source_scope: { product: "firefox", profile: "p", generation: "g" },
  parent: { kind: "unknown" }, original_unix_micros: at * 1000, source_is_local: null,
  url: `https://example.net/${id}`, title: null, favicon_url: null, referrer_url: null,
  transition: "link", at_ms: at, private: false,
});
stored["graphshell.capture.policy.v1"] = { ...globalThis.GraphshellCaptureModel.defaultPolicy(), enabled: true };
stored["graphshell.capture.queue.v1"] = [visit("one", 1), visit("two", 2)];

const { prepareCapture } = await import("../extension-profile.js");
await prepareCapture();
assert.equal(JSON.parse(globalThis.graphshellInitialVisitsJson).length, 2, "the page is handed the queue");

// The host has not landed the batch: nothing is acknowledged.
await settle();
assert.equal((await send({ type: "capture.pull" })).visits.length, 2, "the queue waits for the host");

// The host writes its counters on the Graphshell root, then announces the landed batch.
Object.assign(root.dataset, { captureAccepted: "1", captureDropped: "0", captureBuffered: "1" });
await globalThis.document.dispatch("graphshell-capture-complete");
await settle();
assert.equal((await send({ type: "capture.pull" })).visits.length, 0, "the announcement drains the queue");
assert.equal(status.textContent, "Graph updated · 1 visits · 1 awaiting parents · 0 filtered",
  "the counters are read from the Graphshell root");
console.log("Graphshell capture acknowledgement smoke passed");
