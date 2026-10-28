// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

function event() {
  return {
    listeners: [],
    addListener(listener) {
      this.listeners.push(listener);
    },
  };
}

const actionClicked = event();
const runtimeMessage = event();
const historyVisited = event();
const permissionAdded = event();
const permissionRemoved = event();
const stored = {};
let historyGranted = false;
const fromTime = Date.now() - 2000;
const toTime = Date.now() - 1000;

const chrome = {
  action: { onClicked: actionClicked },
  tabs: { create: async () => {} },
  runtime: {
    getURL: (path) => `chrome-extension://graphshell/${path}`,
    onMessage: runtimeMessage,
  },
  storage: {
    local: {
      async get(keys) {
        return Object.fromEntries(
          keys.filter((key) => key in stored).map((key) => [key, stored[key]]),
        );
      },
      async set(values) {
        Object.assign(stored, values);
      },
    },
  },
  permissions: {
    contains: async () => historyGranted,
    onAdded: permissionAdded,
    onRemoved: permissionRemoved,
  },
  history: {
    onVisited: historyVisited,
    async search() {
      return [
        {
          id: "page-1",
          url: "https://example.net/to?token=secret#part",
          title: "Destination",
          lastVisitTime: toTime,
        },
        {
          id: "page-2",
          url: "https://example.net/from?token=secret",
          title: "Source",
          lastVisitTime: fromTime,
        },
      ];
    },
    async getVisits({ url }) {
      if (url.includes("/from")) {
        return [
          {
            visitId: "from",
            referringVisitId: "",
            transition: "typed",
            visitTime: fromTime,
          },
        ];
      }
      return [
        {
          visitId: "to",
          referringVisitId: "from",
          transition: "link",
          visitTime: toTime,
        },
      ];
    },
  },
};

const context = vm.createContext({
  chrome,
  console,
  URL,
  crypto: { randomUUID: () => "fixture-scope" },
  globalThis: null,
  importScripts() {},
});
context.globalThis = context;
vm.runInContext(
  fs.readFileSync(new URL("./capture-model.js", import.meta.url), "utf8"),
  context,
);
vm.runInContext(
  fs.readFileSync(new URL("./background.js", import.meta.url), "utf8"),
  context,
);

function send(message, listener = runtimeMessage.listeners[0]) {
  return new Promise((resolve) => {
    const keptOpen = listener(message, {}, resolve);
    assert.equal(keptOpen, true);
  }).then((response) => {
    if (response?.error) {
      throw new Error(response.error);
    }
    return response;
  });
}

historyGranted = true;
const status = await send({ type: "capture.status" });
assert.equal(status.permission, true);
const configured = await send({
  type: "capture.configure",
  policy: {
    ...status.policy,
    enabled: true,
    strip_query: true,
  },
});
assert.equal(configured.policy.enabled, true);
assert.equal((await send({ type: "capture.import", days: 7 })).queued, 2);
const pulled = await send({ type: "capture.pull" });
assert.equal(pulled.visits.length, 2);
assert.equal(pulled.visits[1].url, "https://example.net/to");
assert.equal(pulled.visits[1].referrer_url, "https://example.net/from");
assert.equal(pulled.visits[1].transition, "link");
assert.equal(pulled.visits[1].parent.kind, "visit");
assert.equal(pulled.visits[1].parent.visit_id, "from");
assert.equal(pulled.visits[1].source_scope.profile, "fixture-scope");
const fixtureArgument = process.argv.indexOf("--fixture");
if (fixtureArgument >= 0) {
  fs.writeFileSync(process.argv[fixtureArgument + 1], JSON.stringify({ policy: pulled.policy, visits: pulled.visits }, null, 2) + "\n");
}
await send({ type: "capture.ack", ids: pulled.ids });
assert.equal((await send({ type: "capture.pull" })).visits.length, 0);

// Two rapid visits returned by one reconciliation survive, including a
// same-address source parent. Browser HistoryItem.id must never become visitId.
const originalVisits = chrome.history.getVisits;
const liveAt = Date.now() + 10;
chrome.history.getVisits = async ({ url }) => url.includes("/to") ? [
  { visitId: "rapid-1", referringVisitId: "from", transition: "link", visitTime: liveAt },
  { visitId: "rapid-2", referringVisitId: "rapid-1", transition: "reload", visitTime: liveAt + 1 },
] : originalVisits({ url });
historyVisited.listeners[0]({ id: "page-1", url: "https://example.net/to?token=secret", title: "Destination" });
historyVisited.listeners[0]({ id: "page-1", url: "https://example.net/to?token=secret", title: "Destination" });
for (let attempt = 0; attempt < 20; attempt++) {
  if ((await send({ type: "capture.pull" })).visits.length === 2) break;
  await new Promise(setImmediate);
}
const rapid = await send({ type: "capture.pull" });
assert.deepEqual(Array.from(rapid.visits, (visit) => visit.visit_id), ["rapid-1", "rapid-2"]);
assert.equal(rapid.visits[1].parent.visit_id, "rapid-1");
assert.equal(rapid.visits[1].original_unix_micros, (liveAt + 1) * 1000);
await send({ type: "capture.ack", ids: rapid.ids });

// Worker restart restores source scope and the live observation floor.
const restarted = vm.createContext({ chrome, console, URL,
  crypto: { randomUUID() { throw new Error("scope must survive restart"); } }, importScripts() {} });
restarted.globalThis = restarted;
for (const file of ["capture-model.js", "background.js"]) {
  vm.runInContext(fs.readFileSync(new URL(`./${file}`, import.meta.url), "utf8"), restarted);
}
const lastMessage = runtimeMessage.listeners.at(-1);
historyVisited.listeners.at(-1)({ id: "page-1", url: "https://example.net/to?token=secret" });
for (let attempt = 0; attempt < 20; attempt++) {
  if ((await send({ type: "capture.pull" }, lastMessage)).visits.length === 2) break;
  await new Promise(setImmediate);
}
assert.equal((await send({ type: "capture.pull" }, lastMessage)).visits[0].source_scope.profile, "fixture-scope");

// End-time filtering is per visit: a page whose latest visit is after the
// selected range still contributes an older in-range occurrence.
chrome.history.search = async (query) => {
  assert.equal(query.endTime, undefined);
  return [{ id: "page-1", url: "https://example.net/to", lastVisitTime: liveAt + 100 }];
};
chrome.history.getVisits = async () => [
  { visitId: "range-1", referringVisitId: "0", transition: "typed", visitTime: liveAt - 1 },
  { visitId: "range-2", referringVisitId: "range-1", transition: "link", visitTime: liveAt + 100 },
];
const range = await send({ type: "capture.import", start_ms: liveAt - 2, end_ms: liveAt }, lastMessage);
assert.equal(range.queued, 1);
assert.equal(range.coverage.source_completeness, "unknown");

// No destination acquisition exists in the adapter. A large candidate set
// reports saturation and queue loss rather than claiming complete history.
chrome.history.search = async () => Array.from({ length: 512 }, (_, index) => ({ url: `https://example.net/page-${index}` }));
chrome.history.getVisits = async ({ url }) => Array.from({ length: 5 }, (_, index) => ({
  visitId: `${url}-${index}`, referringVisitId: "0", transition: "typed", visitTime: liveAt,
}));
const saturated = await send({ type: "capture.import", start_ms: liveAt - 1, end_ms: liveAt + 1 }, lastMessage);
assert.equal(saturated.coverage.candidate_limit_reached, true);
assert.ok(saturated.coverage.omitted_queue > 0);
assert.equal((await send({ type: "capture.pull" }, lastMessage)).visits.length, 2048);

// Policy, queue, visit map and live-from share one writer order. A write held
// mid-flight is never overwritten by another writer's earlier read.
const POLICY_KEY = "graphshell.capture.policy.v1";
const settle = async () => { for (let turn = 0; turn < 20; turn++) await new Promise(setImmediate); };
function holdNextPolicyWrite() {
  const write = chrome.storage.local.set;
  let release;
  let entered;
  const reached = new Promise((resolve) => { entered = resolve; });
  const gate = new Promise((resolve) => { release = resolve; });
  chrome.storage.local.set = async (values) => {
    if (POLICY_KEY in values) {
      chrome.storage.local.set = write;
      entered();
      await gate;
    }
    return write(values);
  };
  return { reached, release };
}
async function pulledVisit(id) {
  for (let attempt = 0; attempt < 20; attempt++) {
    const pulled = await send({ type: "capture.pull" }, lastMessage);
    const found = pulled.visits.find((visit) => visit.visit_id === id);
    if (found) return found;
    await new Promise(setImmediate);
  }
  return undefined;
}
const raceAt = Date.now() + 100;
const observed = { id: "page-1", url: "https://example.net/to?token=secret", title: "Destination" };
const visitsBeforeRaces = chrome.history.getVisits;
chrome.history.getVisits = async () => [
  { visitId: "race-1", referringVisitId: "0", transition: "link", visitTime: raceAt },
];

const racingPolicy = (await send({ type: "capture.status" }, lastMessage)).policy;
const configureHeld = holdNextPolicyWrite();
const configuring = send({ type: "capture.configure", policy: { ...racingPolicy, segment_size: 2 } }, lastMessage);
await configureHeld.reached;
historyVisited.listeners.at(-1)(observed);
await settle();
configureHeld.release();
await configuring;
assert.equal((await pulledVisit("race-1"))?.visit_id, "race-1", "a configure racing a queued visit keeps the visit");
assert.equal((await send({ type: "capture.status" }, lastMessage)).policy.segment_size, 2);

// A conflicting redelivery is refused alone and counted; the batch around it queues.
chrome.history.getVisits = async () => [
  { visitId: "race-1", referringVisitId: "0", transition: "reload", visitTime: raceAt },
  { visitId: "race-2", referringVisitId: "0", transition: "link", visitTime: raceAt + 1 },
];
historyVisited.listeners.at(-1)(observed);
assert.equal((await pulledVisit("race-2"))?.visit_id, "race-2", "the rest of the batch queues");
assert.equal((await pulledVisit("race-1")).transition, "link", "the first visit for an identity wins");
assert.equal((await send({ type: "capture.status" }, lastMessage)).coverage.refused_conflicts, 1);
chrome.history.getVisits = visitsBeforeRaces;

// A permission removal waits for the configure in flight and disables on top of it.
const removalHeld = holdNextPolicyWrite();
const reconfiguring = send({
  type: "capture.configure", policy: { ...racingPolicy, excluded_origins: ["https://blocked.example"] },
}, lastMessage);
await removalHeld.reached;
permissionRemoved.listeners.at(-1)({ permissions: ["history"] });
await settle();
removalHeld.release();
await reconfiguring;
await settle();
const afterRemoval = (await send({ type: "capture.status" }, lastMessage)).policy;
assert.equal(afterRemoval.enabled, false, "the removal is not overwritten by the configure it raced");
assert.deepEqual(Array.from(afterRemoval.excluded_origins), ["https://blocked.example"], "the removal keeps the configure's policy");

// The Firefox browser namespace runs the same production adapters with an
// independent profile store. This is API simulation, not a headed receipt.
const firefoxStored = {};
const firefoxMessages = event();
const firefox = { ...chrome,
  runtime: { ...chrome.runtime, getURL: (path) => `moz-extension://fixture/${path}`, onMessage: firefoxMessages },
  storage: { local: {
    async get(keys) { return Object.fromEntries(keys.filter((key) => key in firefoxStored).map((key) => [key, firefoxStored[key]])); },
    async set(values) { Object.assign(firefoxStored, values); },
  } },
};
const firefoxContext = vm.createContext({ browser: firefox, console, URL,
  crypto: { randomUUID: () => "firefox-fixture" }, importScripts() {} });
firefoxContext.globalThis = firefoxContext;
for (const file of ["capture-model.js", "background.js"]) {
  vm.runInContext(fs.readFileSync(new URL(`./${file}`, import.meta.url), "utf8"), firefoxContext);
}
const firefoxListener = firefoxMessages.listeners[0];
const firefoxStatus = await send({ type: "capture.status" }, firefoxListener);
assert.equal(firefoxStatus.source_scope.product, "firefox");
await send({ type: "capture.configure", policy: { ...firefoxStatus.policy, enabled: true } }, firefoxListener);
await send({ type: "capture.import", start_ms: liveAt - 1, end_ms: liveAt + 1 }, firefoxListener);
assert.equal((await send({ type: "capture.pull" }, firefoxListener)).visits[0].source_scope.product, "firefox");
console.log("Graphshell capture background smoke passed");
