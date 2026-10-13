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
const SOURCE = extensionApi.runtime.getURL("").startsWith("moz-extension:")
  ? "firefox"
  : "chromium";
let queueMutation = Promise.resolve();
let sourceScopePromise;
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
    coverage: stored[COVERAGE_KEY] ?? { omitted_queue: 0, candidate_limit_reached: false },
    liveFrom: stored[LIVE_FROM_KEY] ?? null,
  };
}

function serializedQueueMutation(operation) {
  const next = queueMutation.then(operation, operation);
  queueMutation = next.catch(() => {});
  return next;
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
    const queue = model.mergeQueue(state.queue, visits);
    const identities = new Set([...state.queue, ...visits].map(model.queueKey));
    const omitted = Math.max(0, identities.size - queue.length);
    const visitMap = { ...state.visitMap };
    for (const visit of visits) {
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
      [COVERAGE_KEY]: { ...state.coverage, omitted_queue: state.coverage.omitted_queue + omitted },
    });
    return visits.length;
  });
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
  await queueVisits(visits.filter((visit) => Number.isFinite(visit.visitTime) && visit.visitId
      && visit.visitTime >= (state.liveFrom ?? Date.now()))
    .map((visit) => visitObservation(historyItem, visit, scope, state.visitMap)));
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

async function handleMessage(message) {
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
    const policy = model.normalizePolicy(message.policy);
    const state = await storedCaptureState();
    const queue = state.queue
      .map((visit) =>
        model.sanitizeVisit(visit, {
          ...policy,
          enabled: true,
        }),
      )
      .filter(Boolean);
    const visitMap = Object.fromEntries(Object.entries(state.visitMap).flatMap(([id, value]) => {
      const accepted = model.sanitizeVisit({ ...value, private: false }, { ...policy, enabled: true });
      return accepted ? [[id, { url: accepted.url, at_ms: value.at_ms }]] : [];
    }));
    await extensionApi.storage.local.set({
      [POLICY_KEY]: policy,
      [LIVE_FROM_KEY]: policy.enabled ? (state.policy.enabled ? state.liveFrom ?? Date.now() : Date.now()) : null,
      [QUEUE_KEY]: model.mergeQueue([], queue),
      [VISIT_MAP_KEY]: visitMap,
    });
    return { policy };
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
    const acknowledged = new Set(Array.isArray(message.ids) ? message.ids : []);
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
  throw new Error(`Unknown Graphshell extension message: ${message?.type}`);
}

extensionApi.runtime.onMessage.addListener((message, _sender, respond) => {
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

extensionApi.permissions.onRemoved.addListener((removed) => {
  if (!removed.permissions?.includes("history")) {
    return;
  }
  storedCaptureState()
    .then((state) =>
      extensionApi.storage.local.set({
        [POLICY_KEY]: { ...state.policy, enabled: false },
      }),
    )
    .catch((error) => console.error("Graphshell permission removal", error));
});
