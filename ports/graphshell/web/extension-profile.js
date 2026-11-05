// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

const extensionApi = globalThis.browser ?? globalThis.chrome;
const model = globalThis.GraphshellCaptureModel;
const panel = document.querySelector("#capture-panel");
const statusNode = document.querySelector("#capture-status");
const rangeNode = document.querySelector("#capture-range");
const stripQueryNode = document.querySelector("#capture-strip-query");
const exclusionsNode = document.querySelector("#capture-exclusions");
const enableButton = document.querySelector("#capture-enable");
const saveButton = document.querySelector("#capture-save");
const disableButton = document.querySelector("#capture-disable");
const historyRangeNode = document.querySelector("#history-filter-range");
const historyPersonaNode = document.querySelector("#history-filter-persona");
const historyDeviceNode = document.querySelector("#history-filter-device");
const historyFilterButton = document.querySelector("#history-filter-apply");
const historyResultStatusNode = document.querySelector("#history-result-status");
const historyForgetAddressNode = document.querySelector("#history-forget-address");
const historyForgetObjectNode = document.querySelector("#history-forget-object");
const historyForgetButton = document.querySelector("#history-forget");
const HISTORY_FILTER_STATE_KEY = "graphshell.history.filter.controls.v1";
const HISTORY_FORGET_KEY = "graphshell.history.forget.pending.v1";
const WRITER_LOCK = "graphshell.local-profile.writer.v1";
const VIEWS_CHANNEL = "graphshell.local-profile.views.v1";
// How long a follower leaves a vacant writer lock alone after the writer
// announces it is unloading, so a reloaded page can claim it back first.
const RELOAD_GRACE_MS = 3000;
const LOCAL_PROFILE_GLOBAL = "graphshellLocalProfileJson";
let pulledIds = [];
let currentPolicy = model.defaultPolicy();

function setStatus(message, state = "ready") {
  if (!statusNode) return;
  statusNode.textContent = message;
  statusNode.dataset.state = state;
}

// Firefox rejects a message that reaches its event page while it suspends;
// the next attempt wakes it. Other failures are not retried.
const NO_RECEIVER = /Receiving end does not exist/;

async function send(message) {
  let response;
  for (let attempt = 0; ; attempt += 1) {
    try {
      response = await extensionApi.runtime.sendMessage(message);
      break;
    } catch (error) {
      if (attempt >= 3 || !NO_RECEIVER.test(String(error))) throw error;
      await new Promise((resolve) => setTimeout(resolve, 100 * (attempt + 1)));
    }
  }
  if (response?.error) {
    throw new Error(response.error);
  }
  return response;
}

function policyFromControls(current, enabled = current.enabled) {
  return model.normalizePolicy({
    ...current,
    enabled,
    strip_query: stripQueryNode.checked,
    excluded_origins: exclusionsNode.value
      .split(/[\n,]/)
      .map((origin) => origin.trim())
      .filter(Boolean),
  });
}

function showPolicy(policy) {
  stripQueryNode.checked = policy.strip_query;
  exclusionsNode.value = policy.excluded_origins.join("\n");
}

function sessionJson(key, fallback) {
  try {
    const value = sessionStorage.getItem(key);
    return value ? JSON.parse(value) : fallback;
  } catch {
    return fallback;
  }
}

function historyFilterState() {
  const stored = sessionJson(HISTORY_FILTER_STATE_KEY, {});
  return {
    days: Number.isInteger(stored.days) ? stored.days : 7,
    persona: typeof stored.persona === "string" ? stored.persona : "",
    device: typeof stored.device === "string" ? stored.device : "",
  };
}

function showHistoryFilter(state) {
  if (!historyRangeNode || !historyPersonaNode || !historyDeviceNode) return;
  historyRangeNode.value = String(state.days);
  historyPersonaNode.value = state.persona;
  historyDeviceNode.value = state.device;
}

function applyHistoryFilter() {
  const state = {
    days: Number.parseInt(historyRangeNode.value, 10),
    persona: historyPersonaNode.value.trim(),
    device: historyDeviceNode.value.trim(),
  };
  sessionStorage.setItem(HISTORY_FILTER_STATE_KEY, JSON.stringify(state));
  historyResultStatusNode.textContent = "Applying authority filter…";
  location.reload();
}

function forgetHistory() {
  const request = model.forgetRequest(
    historyForgetAddressNode.value,
    historyForgetObjectNode.checked,
    currentPolicy,
  );
  if (!request) {
    historyResultStatusNode.textContent =
      "Enter a stored HTTP or HTTPS address to forget.";
    historyResultStatusNode.dataset.state = "failed";
    return;
  }
  const objectClause = request.remove_object
    ? " and remove its capture-created graph object"
    : "";
  if (
    !globalThis.confirm(
      `Forget all stored browser history for ${request.url}${objectClause}?`,
    )
  ) {
    return;
  }
  sessionStorage.setItem(HISTORY_FORGET_KEY, JSON.stringify(request));
  historyResultStatusNode.textContent = "Forgetting stored history…";
  location.reload();
}

function publishHistoryInputs() {
  const state = historyFilterState();
  showHistoryFilter(state);
  globalThis.graphshellHistoryFilterJson = JSON.stringify(
    model.historyFilterFromControls(
      state.days,
      state.persona,
      state.device,
    ),
  );
  const pendingForget = sessionStorage.getItem(HISTORY_FORGET_KEY);
  if (pendingForget) {
    globalThis.graphshellHistoryForgetJson = pendingForget;
  }
}

function finishHistoryControls() {
  sessionStorage.removeItem(HISTORY_FORGET_KEY);
  if (!historyResultStatusNode) return;
  const error = document.body.dataset.historyError;
  const count = Number(document.body.dataset.historyResultCount || 0);
  const attempted = document.body.dataset.historyForgetAttempted === "true";
  const forgotten = Number(document.body.dataset.historyForgotten || 0);
  if (error) {
    historyResultStatusNode.textContent = `History action failed: ${error}`;
    historyResultStatusNode.dataset.state = "failed";
  } else if (forgotten > 0) {
    historyResultStatusNode.textContent =
      `Forgot ${forgotten} browsing trace(s) · ${count} matching visit(s) remain`;
  } else if (attempted) {
    historyResultStatusNode.textContent =
      `No stored browsing traces matched · ${count} matching visit(s) remain`;
  } else {
    historyResultStatusNode.textContent = `${count} matching stored visit(s)`;
  }
}

async function refreshStatus() {
  const status = await send({ type: "capture.status" });
  currentPolicy = status.policy;
  if (!panel) return status;
  showPolicy(status.policy);
  enableButton.disabled = status.permission && status.policy.enabled;
  disableButton.disabled = !status.permission && !status.policy.enabled;
  setStatus(
    status.permission && status.policy.enabled
      ? `Capture enabled · ${status.queued} visit(s) waiting for Graphshell`
        + (status.coverage?.omitted_queue ? ` · ${status.coverage.omitted_queue} omitted by queue limit` : "")
        + (status.coverage?.refused_conflicts ? ` · ${status.coverage.refused_conflicts} refused as conflicting` : "")
        + (status.coverage?.candidate_limit_reached ? " · history candidate limit reached" : "")
      : "Capture is off. Graphshell has not requested browser history access.",
  );
  return status;
}

async function enableCapture() {
  setStatus("Waiting for browser permission…", "working");
  const granted = await extensionApi.permissions.request({
    permissions: ["history"],
  });
  if (!granted) {
    setStatus("Browser history access was not granted.", "failed");
    return;
  }
  const current = (await send({ type: "capture.status" })).policy;
  const policy = policyFromControls(current, true);
  await send({ type: "capture.configure", policy });
  const days = Number.parseInt(rangeNode.value, 10);
  const imported =
    days > 0
      ? await send({ type: "capture.import", days })
      : { queued: 0 };
  setStatus(`Capture enabled · ${imported.queued} imported visit(s)`
    + (imported.coverage?.candidate_limit_reached ? " · partial candidate range" : ""), "working");
  location.reload();
}

async function savePolicy() {
  const status = await send({ type: "capture.status" });
  const policy = policyFromControls(status.policy);
  await send({ type: "capture.configure", policy });
  setStatus("Privacy settings saved. They apply before visits enter the queue.");
}

async function disableCapture() {
  const removed = await extensionApi.permissions.remove({
    permissions: ["history"],
  });
  const status = await send({ type: "capture.status" });
  await send({
    type: "capture.configure",
    policy: { ...status.policy, enabled: false },
  });
  setStatus(
    removed
      ? "Capture stopped and browser history permission removed."
      : "Capture stopped.",
  );
  await refreshStatus();
}

// The Graphshell root carries the host's capture counters, as `graphshellReceipt` reads them.
const graphshellRoot = () => document.querySelector("graphshell-view, graphshell-tree");
const LIVE_BATCH_EVENT = "graphshell-capture-batch";
const LIVE_BATCH_COMPLETE_EVENT = "graphshell-capture-batch-complete";
let liveBatch = null;
let livePullAgain = false;
let liveSequence = 0;

// Visits queued while this view stays open. One batch is in flight at a
// time; the queue keeps it until the graph reports it committed.
async function pullLive() {
  if (liveBatch) {
    livePullAgain = true;
    return;
  }
  const pulled = await send({ type: "capture.pull" });
  if (!pulled.policy.enabled || pulled.visits.length === 0) return;
  liveSequence += 1;
  liveBatch = { batch: liveSequence, ids: pulled.ids };
  document.dispatchEvent(new CustomEvent(LIVE_BATCH_EVENT, {
    detail: JSON.stringify({ batch: liveSequence, policy: pulled.policy, visits: pulled.visits }),
  }));
}

async function finishLive() {
  const counts = graphshellRoot()?.dataset ?? {};
  if (!liveBatch || Number(counts.captureBatch) !== liveBatch.batch) return;
  const { ids } = liveBatch;
  liveBatch = null;
  if (counts.captureBatchError) {
    // Not acknowledged: the next notice or open pulls it again.
    livePullAgain = false;
    setStatus(`Capture not saved · ${counts.captureBatchError} · the visits stay queued`, "failed");
    return;
  }
  await send({ type: "capture.ack", ids });
  setStatus(`Graph updated · ${counts.captureBatchAccepted || 0} new visit(s)`
    + ` · ${counts.captureBatchBuffered || 0} awaiting parents · ${counts.captureBatchDropped || 0} filtered`);
  if (livePullAgain) {
    livePullAgain = false;
    await pullLive();
  }
}

function startLiveCapture() {
  const failed = (error) => setStatus(`Live capture failed · ${error}`, "failed");
  document.addEventListener(LIVE_BATCH_COMPLETE_EVENT, () => finishLive().catch(failed));
  extensionApi.runtime.onMessage.addListener((message, sender) => {
    if (sender?.id === extensionApi.runtime.id && message?.type === "capture.available") {
      pullLive().catch(failed);
    }
  });
  // Visits that arrived between the opening pull and now.
  pullLive().catch(failed);
}

async function prepareCapture() {
  // The controls are optional: without them the queue is still pulled and acknowledged.
  if (panel) {
    panel.hidden = false;
    historyFilterButton?.addEventListener("click", applyHistoryFilter);
    historyForgetButton?.addEventListener("click", forgetHistory);
    enableButton.addEventListener("click", () => {
      enableCapture().catch((error) => setStatus(String(error), "failed"));
    });
    saveButton.addEventListener("click", () => {
      savePolicy().catch((error) => setStatus(String(error), "failed"));
    });
    disableButton.addEventListener("click", () => {
      disableCapture().catch((error) => setStatus(String(error), "failed"));
    });
  }

  const status = await refreshStatus();
  const pulled =
    status.permission && status.policy.enabled
      ? await send({ type: "capture.pull" })
      : { policy: status.policy, visits: [], ids: [] };
  pulledIds = pulled.ids;
  globalThis.graphshellCapturePolicyJson = JSON.stringify(pulled.policy);
  globalThis.graphshellInitialVisitsJson = JSON.stringify(pulled.visits);
  currentPolicy = pulled.policy;
  publishHistoryInputs();

  document.addEventListener(
    "graphshell-capture-complete",
    async () => {
      if (pulledIds.length > 0) {
        await send({ type: "capture.ack", ids: pulledIds });
        pulledIds = [];
      }
      const counts = graphshellRoot()?.dataset ?? {};
      const accepted = Number(counts.captureAccepted || 0);
      const dropped = Number(counts.captureDropped || 0);
      const buffered = Number(counts.captureBuffered || 0);
      setStatus(`Graph updated · ${accepted} visits · ${buffered} awaiting parents · ${dropped} filtered`);
      // Only the writer of the installed profile takes visits while open.
      if (!fixtureRoute()) startLiveCapture();
    },
    { once: true },
  );
  document.addEventListener(
    "graphshell-history-controls-complete",
    finishHistoryControls,
    { once: true },
  );
}

// The installed profile's local scope, for the host to attribute work to.
async function publishLocalProfile() {
  globalThis[LOCAL_PROFILE_GLOBAL] = JSON.stringify(await send({ type: "profile.local" }));
}

const fixtureRoute = () => new URLSearchParams(location.search).has("fixture");

async function focusThisView(channel) {
  const tab = await extensionApi.tabs?.getCurrent?.();
  if (!tab) {
    channel.postMessage({ type: "shown", where: "panel" });
    return;
  }
  await extensionApi.tabs.update(tab.id, { active: true });
  await extensionApi.windows?.update?.(tab.windowId, { focused: true });
  channel.postMessage({ type: "shown", where: "tab" });
}

// The writer answers other views and tells them when a save has finished,
// which is when the root's `data-writer-busy` returns to false. It hands over
// only with nothing unsaved: reloading drops this document, the browser then
// grants the queued view, and this one comes back following it. Any other
// unload is announced as `reloading`: followers wait a moment before taking
// the vacant lock, so a reload keeps the role. A close looks the same, and
// only delays the takeover by that moment.
function serveViews() {
  const channel = new BroadcastChannel(VIEWS_CHANNEL);
  let handingOver = false;
  addEventListener("pagehide", () => {
    if (!handingOver) channel.postMessage({ type: "reloading" });
  });
  channel.onmessage = ({ data }) => {
    if (data?.type === "show") {
      focusThisView(channel).catch((error) => console.error("Graphshell focus", error));
    } else if (data?.type === "handover") {
      if (graphshellRoot()?.dataset.writerBusy === "true") {
        channel.postMessage({ type: "busy" });
      } else {
        handingOver = true;
        channel.postMessage({ type: "released" });
        location.reload();
      }
    }
  };
  const root = graphshellRoot();
  let busy = root?.dataset.writerBusy === "true";
  new MutationObserver(() => {
    const now = root.dataset.writerBusy === "true";
    if (busy && !now) channel.postMessage({ type: "committed" });
    busy = now;
  }).observe(root, { attributes: true, attributeFilter: ["data-writer-busy"] });
}

// Drop the observing mount and mount again as the writer, keeping the lock
// this document now holds; the element remounts when it is reconnected.
async function becomeWriter(bar) {
  globalThis.graphshellViewRole = "writer";
  bar.hidden = true;
  await prepareCapture();
  const root = graphshellRoot();
  const parent = root.parentNode;
  const next = root.nextSibling;
  root.remove();
  parent.insertBefore(root, next);
  serveViews();
}

// Another view holds the writer: this one shows the same graph, reads it
// again after each of that view's saves, and can ask to edit here instead.
function follow() {
  const bar = document.querySelector("#writer-standby");
  const status = document.querySelector("#writer-standby-status");
  if (!bar) throw new Error("Graphshell is open in another view of this browser profile");
  globalThis.graphshellViewRole = "follower";
  bar.hidden = false;
  const channel = new BroadcastChannel(VIEWS_CHANNEL);
  let request;
  let holdUntil = 0;
  channel.onmessage = ({ data }) => {
    if (data?.type === "committed") {
      document.dispatchEvent(new Event("graphshell-committed"));
    } else if (data?.type === "shown") {
      status.textContent = data.where === "panel"
        ? "Editing happens in this browser's side panel."
        : "Switched to the view that edits.";
    } else if (data?.type === "reloading") {
      holdUntil = Date.now() + RELOAD_GRACE_MS;
    } else if (data?.type === "busy") {
      request?.abort();
      request = null;
      status.textContent = "That view has a change being saved or awaiting retry. Finish it there first.";
    }
  };
  // A view shown again may have missed a notice while hidden.
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && globalThis.graphshellViewRole === "follower") {
      document.dispatchEvent(new Event("graphshell-committed"));
    }
  });
  document.querySelector("#writer-show").addEventListener("click", () => {
    channel.postMessage({ type: "show" });
  });
  const granted = (lock) => {
    if (!lock) return null;
    clearInterval(vacancy);
    channel.close();
    becomeWriter(bar).catch((error) => {
      bar.hidden = false;
      status.textContent = `Could not start editing here · ${error}`;
    });
    return new Promise(() => {});
  };
  document.querySelector("#writer-take").addEventListener("click", () => {
    if (request) return;
    request = new AbortController();
    status.textContent = "Asking the other view to hand over…";
    navigator.locks.request(WRITER_LOCK, { signal: request.signal }, granted).catch((error) => {
      if (error.name !== "AbortError") status.textContent = String(error);
    });
    channel.postMessage({ type: "handover" });
  });
  // When the writing view closes, the first follower to notice edits instead.
  const vacancy = setInterval(async () => {
    if (request || globalThis.graphshellViewRole !== "follower") return;
    if (Date.now() < holdUntil) return;
    const { held } = await navigator.locks.query();
    if (held.some((lock) => lock.name === WRITER_LOCK)) return;
    navigator.locks.request(WRITER_LOCK, { ifAvailable: true }, granted)
      .catch((error) => console.error("Graphshell writer", error));
  }, 2000);
}

// One view of the installed profile edits its store at a time; others
// follow it. The lock is held for this document's life and released by the
// browser when it goes. Answers "writer" or "follower".
async function claimWriter() {
  if (fixtureRoute()) return "writer";
  if (!navigator.locks) {
    throw new Error("This browser cannot keep Graphshell to one writer");
  }
  const held = await new Promise((resolve) => {
    navigator.locks.request(WRITER_LOCK, { ifAvailable: true }, (lock) => {
      resolve(Boolean(lock));
      return lock ? new Promise(() => {}) : null;
    });
  });
  if (!held) {
    follow();
    return "follower";
  }
  globalThis.graphshellViewRole = "writer";
  // The root exists before mounting; its busy flag appears with the mount.
  serveViews();
  return "writer";
}

export { claimWriter, prepareCapture, publishLocalProfile };
