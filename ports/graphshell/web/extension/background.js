// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

if (!globalThis.GraphshellCaptureModel && typeof importScripts === "function") {
  importScripts("capture-model.js");
}

const extensionApi = globalThis.browser ?? globalThis.chrome;
const model = globalThis.GraphshellCaptureModel;
const POLICY_KEY = "graphshell.capture.policy.v1";
const QUEUE_KEY = "graphshell.capture.queue.v1";
const VISIT_MAP_KEY = "graphshell.capture.visit-map.v1";
const SOURCE_SCOPE_KEY = "graphshell.capture.source-scope.v1";
const COVERAGE_KEY = "graphshell.capture.coverage.v1";
const LIVE_FROM_KEY = "graphshell.capture.live-from.v1";
// "Keep page text": its own control, queue and counters, apart from the visit queue.
const KEEP_POLICY_KEY = "graphshell.keep.policy.v1";
const KEEP_QUEUE_KEY = "graphshell.keep.queue.v1";
const KEEP_STATE_KEY = "graphshell.keep.state.v1";
const KEEP_PERMISSIONS = { permissions: ["scripting"], origins: ["<all_urls>"] };
// The installed profile's local, unassociated scope: no persona, no custody.
const LOCAL_PROFILE_KEY = "graphshell.local-profile.v1";
const LOCAL_PROFILE_SCHEMA = "graphshell.local-profile/v1";
// No message from a packaged page needs more; larger ones are refused unread.
const MAX_MESSAGE_CHARS = 64 * 1024;
const EXTENSION_ORIGIN = extensionApi.runtime.getURL("");
const SOURCE = extensionApi.runtime.getURL("").startsWith("moz-extension:")
  ? "firefox"
  : "chromium";
let queueMutation = Promise.resolve();
let sourceScopePromise;
let localProfilePromise;
const addressReconciliation = new Map();

function sourceScope() {
  sourceScopePromise ??= (async () => {
    const stored = await extensionApi.storage.local.get([SOURCE_SCOPE_KEY]);
    const scope = stored[SOURCE_SCOPE_KEY] ?? {
      product: SOURCE, profile: crypto.randomUUID(), generation: crypto.randomUUID(),
    };
    await extensionApi.storage.local.set({ [SOURCE_SCOPE_KEY]: scope });
    return scope;
  })();
  return sourceScopePromise;
}

function localProfile() {
  localProfilePromise ??= (async () => {
    const stored = await extensionApi.storage.local.get([LOCAL_PROFILE_KEY]);
    if (stored[LOCAL_PROFILE_KEY]?.schema === LOCAL_PROFILE_SCHEMA) return stored[LOCAL_PROFILE_KEY];
    const profile = { schema: LOCAL_PROFILE_SCHEMA, id: crypto.randomUUID(), browser: SOURCE };
    await extensionApi.storage.local.set({ [LOCAL_PROFILE_KEY]: profile });
    return profile;
  })();
  return localProfilePromise;
}

// Only this extension's own pages may drive capture. It registers no content
// scripts: page text is read by an on-demand injection, only while the person
// has switched "Keep page text" on and granted its optional access.
function fromPackagedPage(sender) {
  return sender?.id === extensionApi.runtime.id
    && typeof sender.url === "string" && sender.url.startsWith(EXTENSION_ORIGIN);
}

extensionApi.action.onClicked.addListener(() => {
  extensionApi.tabs.create({
    url: extensionApi.runtime.getURL("graph.html"),
  });
});

async function storedCaptureState() {
  const stored = await extensionApi.storage.local.get([
    POLICY_KEY,
    QUEUE_KEY,
    VISIT_MAP_KEY,
    COVERAGE_KEY,
    LIVE_FROM_KEY,
  ]);
  return {
    policy: model.normalizePolicy(stored[POLICY_KEY]),
    queue: Array.isArray(stored[QUEUE_KEY]) ? stored[QUEUE_KEY] : [],
    visitMap:
      stored[VISIT_MAP_KEY] && typeof stored[VISIT_MAP_KEY] === "object"
        ? stored[VISIT_MAP_KEY]
        : {},
    coverage: stored[COVERAGE_KEY]
      ?? { omitted_queue: 0, refused_conflicts: 0, candidate_limit_reached: false },
    liveFrom: stored[LIVE_FROM_KEY] ?? null,
  };
}

// The one writer order for the policy, queue, visit map, coverage and
// live-from: every write to them runs here, after reading the state it changes.
function serializedQueueMutation(operation) {
  const next = queueMutation.then(operation, operation);
  queueMutation = next.catch(() => {});
  return next;
}

function withRefused(coverage, refused) {
  return { ...coverage, refused_conflicts: (coverage.refused_conflicts ?? 0) + refused.length };
}

async function queueVisits(rawVisits) {
  return serializedQueueMutation(async () => {
    const state = await storedCaptureState();
    if (!state.policy.enabled) {
      return 0;
    }
    const visits = rawVisits
      .map((visit) => model.sanitizeVisit(visit, state.policy))
      .filter(Boolean);
    const { queue, refused } = model.mergeQueue(state.queue, visits);
    const admitted = visits.filter((visit) => !refused.includes(visit));
    const identities = new Set([...state.queue, ...admitted].map(model.queueKey));
    const omitted = Math.max(0, identities.size - queue.length);
    const visitMap = { ...state.visitMap };
    for (const visit of admitted) {
      if (visit.visit_id) {
        visitMap[visit.visit_id] = { url: visit.url, at_ms: visit.at_ms };
      }
    }
    const retainedVisitMap = Object.fromEntries(
      Object.entries(visitMap)
        .sort((left, right) => left[1].at_ms - right[1].at_ms)
        .slice(-512),
    );
    await extensionApi.storage.local.set({
      [QUEUE_KEY]: queue,
      [VISIT_MAP_KEY]: retainedVisitMap,
      [COVERAGE_KEY]: {
        ...withRefused(state.coverage, refused),
        omitted_queue: state.coverage.omitted_queue + omitted,
      },
    });
    return admitted.length;
  });
}

// Tell an open writer view that visits are waiting. With no view open the
// browser rejects the send; the queue keeps them for the next open.
function announceQueued() {
  try {
    Promise.resolve(extensionApi.runtime.sendMessage({ type: "capture.available" }))
      .catch(() => {});
  } catch {
    // No receiver: the visits stay queued.
  }
}

function visitObservation(page, visit, scope, visitUrls) {
  const parentId = visit.referringVisitId && visit.referringVisitId !== "0"
    ? String(visit.referringVisitId) : null;
  const at = visit.visitTime;
  return {
    source: SOURCE, source_scope: scope, visit_id: visit.visitId ?? null,
    parent: parentId ? { kind: "visit", visit_id: parentId } : { kind: "unknown" },
    url: page.url, title: page.title ?? null, favicon_url: null,
    referrer_url: parentId ? visitUrls[parentId]?.url ?? null : null,
    transition: visit.transition ?? "unknown", at_ms: Math.trunc(at),
    original_unix_micros: Math.trunc(at * 1000),
    source_is_local: typeof visit.isLocal === "boolean" ? visit.isLocal : null,
    private: false,
  };
}

async function reconcileAddress(historyItem) {
  const state = await storedCaptureState();
  if (!state.policy.enabled || !historyItem.url) {
    return;
  }
  const visits = await extensionApi.history.getVisits({ url: historyItem.url });
  const scope = await sourceScope();
  // Reconcile every available visit. Durable admission deduplicates retries;
  // a latest timestamp cannot disambiguate rapid or out-of-order visits.
  const admitted = await queueVisits(visits.filter((visit) => Number.isFinite(visit.visitTime)
      && visit.visitId && visit.visitTime >= (state.liveFrom ?? Date.now()))
    .map((visit) => visitObservation(historyItem, visit, scope, state.visitMap)));
  if (admitted > 0) announceQueued();
}

function liveVisit(historyItem) {
  const previous = addressReconciliation.get(historyItem.url) ?? Promise.resolve();
  const current = previous.catch(() => {}).then(() => reconcileAddress(historyItem));
  addressReconciliation.set(historyItem.url, current);
  current.finally(() => {
    if (addressReconciliation.get(historyItem.url) === current) addressReconciliation.delete(historyItem.url);
  }).catch(() => {});
  return current;
}

async function importHistory(days, range = {}) {
  const state = await storedCaptureState();
  if (!state.policy.enabled) {
    return 0;
  }
  const startTime = Number.isFinite(range.start_ms) ? range.start_ms
    : Date.now() - Math.max(1, Math.min(365, days)) * 86_400_000;
  const endTime = Number.isFinite(range.end_ms) ? range.end_ms : Date.now();
  if (startTime < 0 || endTime <= startTime) throw new Error("Invalid history import range");
  const pages = await extensionApi.history.search({
    text: "",
    startTime,
    maxResults: 512,
  });
  const pageVisits = await Promise.all(
    pages
      .filter((page) => page.url)
      .map(async (page) => ({
        page,
        visits: await extensionApi.history.getVisits({ url: page.url }),
      })),
  );
  const flattened = pageVisits
    .flatMap(({ page, visits }) =>
      visits
        .filter((visit) => visit.visitId && Number.isFinite(visit.visitTime)
          && visit.visitTime >= startTime && visit.visitTime < endTime)
        .map((visit) => ({ page, visit })),
    )
    .sort((left, right) => left.visit.visitTime - right.visit.visitTime);
  const visitUrls = Object.fromEntries(
    pageVisits.flatMap(({ page, visits }) => visits.map((visit) =>
      [visit.visitId, { url: page.url, at_ms: visit.visitTime }])),
  );
  const scope = await sourceScope();
  const queued = await queueVisits(flattened.map(({ page, visit }) =>
    {
      const observation = visitObservation(page, visit, scope, visitUrls);
      const parent = visitUrls[visit.referringVisitId];
      if (observation.parent.kind === "visit" && parent
          && (parent.at_ms < startTime || parent.at_ms >= endTime)) {
        observation.parent.kind = "outside_range";
      }
      return observation;
    }));
  return serializedQueueMutation(async () => {
    const state = await storedCaptureState();
    const coverage = { ...state.coverage, start_ms: startTime, end_ms: endTime,
      candidate_limit_reached: pages.length >= 512,
      source_completeness: "unknown", candidate_pages: pages.length };
    await extensionApi.storage.local.set({ [COVERAGE_KEY]: coverage });
    return { queued, coverage };
  });
}

function configureCapture(requested) {
  return serializedQueueMutation(async () => {
    const policy = model.normalizePolicy(requested);
    const state = await storedCaptureState();
    const retained = state.queue
      .map((visit) => model.sanitizeVisit(visit, { ...policy, enabled: true }))
      .filter(Boolean);
    const { queue, refused } = model.mergeQueue([], retained);
    const visitMap = Object.fromEntries(Object.entries(state.visitMap).flatMap(([id, value]) => {
      const accepted = model.sanitizeVisit({ ...value, private: false }, { ...policy, enabled: true });
      return accepted ? [[id, { url: accepted.url, at_ms: value.at_ms }]] : [];
    }));
    await extensionApi.storage.local.set({
      [POLICY_KEY]: policy,
      [LIVE_FROM_KEY]: policy.enabled ? (state.policy.enabled ? state.liveFrom ?? Date.now() : Date.now()) : null,
      [QUEUE_KEY]: queue,
      [VISIT_MAP_KEY]: visitMap,
      ...(refused.length ? { [COVERAGE_KEY]: withRefused(state.coverage, refused) } : {}),
    });
    return { policy };
  });
}

// ---- Keep page text ------------------------------------------------------
// Queued like visits: the worker only extracts and queues; it never opens
// IndexedDB. The writer view drains the queue under the writer lock and
// records each page into Eidetic's PageTextStore.

function emptyKeepState() {
  return { kept: 0, unchanged: 0, refused: 0, failed: 0, omitted_queue: 0, truncated: 0,
    skipped: 0, last_error: null, last_at_ms: null };
}

async function storedKeepState() {
  const stored = await extensionApi.storage.local.get([KEEP_POLICY_KEY, KEEP_QUEUE_KEY, KEEP_STATE_KEY]);
  return {
    keep: model.normalizeKeepPolicy(stored[KEEP_POLICY_KEY]),
    queue: Array.isArray(stored[KEEP_QUEUE_KEY]) ? stored[KEEP_QUEUE_KEY] : [],
    counters: { ...emptyKeepState(), ...(stored[KEEP_STATE_KEY] ?? {}) },
  };
}

function keepPermitted() {
  return extensionApi.permissions.contains(KEEP_PERMISSIONS);
}

// Count something that did not become kept text, with the reason shown to the person.
function noteKeep(change) {
  return serializedQueueMutation(async () => {
    const { counters } = await storedKeepState();
    const next = { ...counters };
    for (const [name, value] of Object.entries(change)) {
      next[name] = typeof value === "number" ? (counters[name] ?? 0) + value : value;
    }
    await extensionApi.storage.local.set({ [KEEP_STATE_KEY]: next });
  });
}

async function configureKeep(requested) {
  const keep = model.normalizeKeepPolicy(requested);
  if (keep.enabled && !(await keepPermitted())) {
    throw new Error("Page text access has not been granted");
  }
  return serializedQueueMutation(async () => {
    const state = await storedKeepState();
    await extensionApi.storage.local.set({
      [KEEP_POLICY_KEY]: keep,
      // Turning it off discards text still waiting; nothing kept after the person said stop.
      [KEEP_QUEUE_KEY]: keep.enabled ? state.queue : [],
      ...(keep.enabled && !state.keep.enabled ? { [KEEP_STATE_KEY]: emptyKeepState() } : {}),
    });
    return { keep };
  });
}

// A page the person loaded in a tab (a background fetch has no tab). Extract
// what the page shows, bound and queue it.
async function keepLoadedPage(tab) {
  if (!tab?.url) return;
  const state = await storedKeepState();
  if (!state.keep.enabled) return;
  if (tab.incognito) {
    await noteKeep({ skipped: 1 });
    return;
  }
  const capture = (await storedCaptureState()).policy;
  if (!model.keepAddress(tab.url, capture)) {
    await noteKeep({ skipped: 1 });
    return;
  }
  if (!(await keepPermitted())) {
    await noteKeep({ failed: 1, last_error: "Page text access is not granted" });
    return;
  }
  let injected;
  try {
    injected = await extensionApi.scripting.executeScript({
      target: { tabId: tab.id }, func: model.extractPageText, args: [state.keep.max_bytes],
    });
  } catch (error) {
    // Pages the browser keeps from extensions (its store, internal pages) land here.
    await noteKeep({ failed: 1, last_error: String(error?.message ?? error) });
    return;
  }
  const read = injected?.[0]?.result;
  if (!read) {
    await noteKeep({ skipped: 1 });
    return;
  }
  const page = model.sanitizeKeptPage({ url: read.url, text: read.text, at_ms: Date.now() },
    state.keep, capture);
  if (!page) {
    await noteKeep({ skipped: 1 });
    return;
  }
  await serializedQueueMutation(async () => {
    const latest = await storedKeepState();
    // Switched off while the page was being read.
    if (!latest.keep.enabled) return;
    const { queue, omitted } = model.mergeKeepQueue(latest.queue, [page]);
    await extensionApi.storage.local.set({
      [KEEP_QUEUE_KEY]: queue,
      [KEEP_STATE_KEY]: { ...latest.counters,
        omitted_queue: latest.counters.omitted_queue + omitted,
        truncated: latest.counters.truncated + (page.truncated ? 1 : 0) },
    });
  });
  announceQueued();
}

async function keepStatus() {
  const state = await storedKeepState();
  return {
    policy: state.keep,
    permission: await keepPermitted(),
    queued: state.queue.length,
    state: state.counters,
  };
}

async function keepPull() {
  const { keep, queue } = await storedKeepState();
  const capture = (await storedCaptureState()).policy;
  const pages = keep.enabled ? model.takeKeepDelivery(queue) : [];
  return {
    policy: { enabled: keep.enabled, max_bytes: keep.max_bytes, excluded_origins: capture.excluded_origins },
    pages: pages.map(({ url, text, at_ms, private: isPrivate }) => ({ url, text, at_ms, private: isPrivate })),
    keys: pages.map(model.keepKey),
  };
}

async function keepAck(message) {
  const keys = Array.isArray(message.keys) ? message.keys : [];
  if (keys.length > model.DEFAULT_QUEUE_LIMIT || keys.some((key) => typeof key !== "string")) {
    throw new Error("Invalid keep acknowledgement");
  }
  const done = new Set(keys);
  const count = (value) => (Number.isInteger(value) && value > 0 ? Math.min(value, 1000) : 0);
  await serializedQueueMutation(async () => {
    const state = await storedKeepState();
    await extensionApi.storage.local.set({
      [KEEP_QUEUE_KEY]: state.queue.filter((page) => !done.has(model.keepKey(page))),
      [KEEP_STATE_KEY]: { ...state.counters,
        kept: state.counters.kept + count(message.kept),
        unchanged: state.counters.unchanged + count(message.unchanged),
        refused: state.counters.refused + count(message.refused),
        last_error: null, last_at_ms: Date.now() },
    });
  });
  return { acknowledged: done.size };
}

// The writer could not record the pages (a full store, for one): they stay queued.
async function keepFail(message) {
  await noteKeep({ failed: 1, last_error: String(message.error ?? "unknown").slice(0, 300) });
  return { recorded: true };
}

async function handleMessage(message) {
  if (message?.type === "profile.local") {
    return localProfile();
  }
  if (message?.type === "capture.status") {
    const state = await storedCaptureState();
    return {
      policy: state.policy,
      source_scope: await sourceScope(),
      queued: state.queue.length,
      coverage: state.coverage,
      permission: await extensionApi.permissions.contains({
        permissions: ["history"],
      }),
    };
  }
  if (message?.type === "capture.configure") {
    return configureCapture(message.policy);
  }
  if (message?.type === "capture.import") {
    return await importHistory(Number(message.days) || 1, message);
  }
  if (message?.type === "capture.pull") {
    const state = await storedCaptureState();
    return {
      policy: state.policy,
      visits: state.queue,
      ids: state.queue.map(model.queueKey),
    };
  }
  if (message?.type === "capture.ack") {
    const ids = Array.isArray(message.ids) ? message.ids : [];
    if (ids.length > model.DEFAULT_QUEUE_LIMIT || ids.some((id) => typeof id !== "string")) {
      throw new Error("Invalid capture acknowledgement");
    }
    const acknowledged = new Set(ids);
    await serializedQueueMutation(async () => {
      const state = await storedCaptureState();
      await extensionApi.storage.local.set({
        [QUEUE_KEY]: state.queue.filter(
          (visit) => !acknowledged.has(model.queueKey(visit)),
        ),
      });
    });
    return { acknowledged: acknowledged.size };
  }
  if (message?.type === "keep.status") return keepStatus();
  if (message?.type === "keep.configure") return configureKeep(message.policy);
  if (message?.type === "keep.pull") return keepPull();
  if (message?.type === "keep.ack") return keepAck(message);
  if (message?.type === "keep.fail") return keepFail(message);
  throw new Error(`Unknown Graphshell extension message: ${message?.type}`);
}

extensionApi.runtime.onMessage.addListener((message, sender, respond) => {
  if (!fromPackagedPage(sender)) {
    respond({ error: "Graphshell accepts messages from its own pages only" });
    return false;
  }
  let size;
  try {
    size = JSON.stringify(message ?? null).length;
  } catch {
    size = Infinity;
  }
  if (size > MAX_MESSAGE_CHARS) {
    respond({ error: "Graphshell extension message is too large" });
    return false;
  }
  handleMessage(message).then(
    (result) => respond(result),
    (error) => respond({ error: String(error) }),
  );
  return true;
});

let historyListenerInstalled = false;

function installHistoryListener() {
  if (historyListenerInstalled || !extensionApi.history?.onVisited) {
    return;
  }
  extensionApi.history.onVisited.addListener((historyItem) => {
    liveVisit(historyItem).catch((error) => console.error("Graphshell capture", error));
  });
  historyListenerInstalled = true;
}

installHistoryListener();

extensionApi.permissions.onAdded.addListener((added) => {
  if (added.permissions?.includes("history")) {
    installHistoryListener();
  }
});

function stopKeepingText() {
  return serializedQueueMutation(async () => {
    const state = await storedKeepState();
    if (!state.keep.enabled) return;
    await extensionApi.storage.local.set({
      [KEEP_POLICY_KEY]: { ...state.keep, enabled: false },
      [KEEP_QUEUE_KEY]: [],
    });
  }).catch((error) => console.error("Graphshell keep permission removal", error));
}

extensionApi.permissions.onRemoved.addListener((removed) => {
  // Page text needs script injection and site access; losing either ends it.
  if (removed.permissions?.includes("scripting") || removed.origins?.length) stopKeepingText();
  if (!removed.permissions?.includes("history")) {
    return;
  }
  serializedQueueMutation(async () => {
    const state = await storedCaptureState();
    await extensionApi.storage.local.set({ [POLICY_KEY]: { ...state.policy, enabled: false } });
  }).catch((error) => console.error("Graphshell permission removal", error));
});

// Registered at top level so a suspended worker wakes for it. Only a completed
// load in a tab is read; the control and its access are checked on every one.
extensionApi.tabs?.onUpdated?.addListener((tabId, change, tab) => {
  if (change.status === "complete") {
    keepLoadedPage({ ...tab, id: tabId }).catch((error) => console.error("Graphshell keep text", error));
  }
});
