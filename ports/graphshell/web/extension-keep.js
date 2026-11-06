// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// The "Page text" control of the extension's workspace: whether the text of
// pages the person loads is kept, its permission, and its state. It is
// separate from live remembering (the History panel). The worker queues
// extracted text; the writer view hands it to the Rust app (event detail
// `keep`) and acknowledges it only after the app has recorded it into
// Eidetic's PageTextStore.

const extensionApi = globalThis.browser ?? globalThis.chrome;
// Reading a loaded page needs on-demand injection and access to its site.
// Both are optional and asked for only when the person turns this on.
const ACCESS = { permissions: ["scripting"], origins: ["<all_urls>"] };
const SIZES = [32 * 1024, 128 * 1024, 256 * 1024];

const node = (id) => document.querySelector(id);
let send = null;

function setStatus(message, state = "ready") {
  const status = node("#keep-status");
  if (!status) return;
  status.textContent = message;
  status.dataset.state = state;
}

function megabytes(bytes) {
  return `${(bytes / 1048576).toFixed(bytes < 10485760 ? 1 : 0)} MB`;
}

// Browser persistence state and quota, as the browser reports them.
async function showStorage() {
  const line = node("#keep-storage");
  if (!line) return;
  try {
    const estimate = await navigator.storage?.estimate?.();
    const persisted = await navigator.storage?.persisted?.();
    line.textContent = estimate?.quota
      ? `Browser storage: ${megabytes(estimate.usage ?? 0)} used of ${megabytes(estimate.quota)}`
        + (persisted === true ? " · protected from automatic clearing"
          : persisted === false ? " · the browser may clear it under storage pressure" : "")
      : "Browser storage use is not reported by this browser.";
  } catch {
    line.textContent = "Browser storage use is not reported by this browser.";
  }
}

function describe(status) {
  const { state } = status;
  const counts = [`${state.kept} kept`, `${status.queued} waiting`, `${state.unchanged} unchanged`];
  if (state.failed) counts.push(`${state.failed} could not be read or saved`);
  if (state.omitted_queue) counts.push(`${state.omitted_queue} omitted by queue limit`);
  if (state.truncated) counts.push(`${state.truncated} cut to the size limit`);
  if (state.refused) counts.push(`${state.refused} refused by the app`);
  if (state.skipped) counts.push(`${state.skipped} skipped (private window, excluded or not a web page)`);
  return counts.join(" · ");
}

export async function refreshKeep() {
  const status = await send({ type: "keep.status" });
  const on = status.policy.enabled && status.permission;
  const enable = node("#keep-enable");
  const disable = node("#keep-disable");
  const size = node("#keep-size");
  if (!enable) return status;
  enable.disabled = on;
  disable.disabled = !on && !status.policy.enabled;
  size.value = String(SIZES.includes(status.policy.max_bytes) ? status.policy.max_bytes : SIZES[1]);
  if (status.policy.enabled && !status.permission) {
    setStatus("Page text is on, but the browser no longer grants access. Nothing is kept until you turn it on again.",
      "failed");
  } else if (on) {
    setStatus(`Keeping page text · ${describe(status)}`);
  } else {
    setStatus("Page text is off. Graphshell has not asked for access to the pages you load.");
  }
  const problem = node("#keep-problem");
  problem.textContent = status.state.last_error ? `Last problem: ${status.state.last_error}` : "";
  problem.hidden = !status.state.last_error;
  await showStorage();
  return status;
}

async function enable() {
  setStatus("Waiting for browser permission…", "working");
  let granted = false;
  try {
    granted = await extensionApi.permissions.request(ACCESS);
  } catch (error) {
    setStatus(`The browser could not ask for access · ${error?.message ?? error}`, "failed");
    return;
  }
  if (!granted) {
    setStatus("Access to the pages you load was not granted. Nothing is kept.", "failed");
    return;
  }
  await send({ type: "keep.configure", policy: { enabled: true, max_bytes: Number(node("#keep-size").value) } });
  setStatus("Page text is on. Pages you load from now on are kept.", "working");
  location.reload();
}

async function disable() {
  await send({ type: "keep.configure", policy: { enabled: false } });
  const removed = await extensionApi.permissions.remove(ACCESS);
  await refreshKeep();
  setStatus(removed
    ? "Page text stopped, waiting text discarded, and access to pages removed. Text already kept stays in your graph."
    : "Page text stopped and waiting text discarded. Text already kept stays in your graph.");
}

async function saveSize() {
  const status = await send({ type: "keep.status" });
  if (!status.policy.enabled) return;
  await send({ type: "keep.configure", policy: { enabled: true, max_bytes: Number(node("#keep-size").value) } });
  setStatus("Size limit saved. It applies to pages loaded from now on.");
}

// Wire the panel and hand the Rust app what was queued while no view was open.
export async function prepareKeep(sendMessage) {
  send = sendMessage;
  const panel = node("#keep-panel");
  if (panel) {
    panel.hidden = false;
    const failed = (error) => setStatus(String(error), "failed");
    node("#keep-enable").addEventListener("click", () => enable().catch(failed));
    node("#keep-disable").addEventListener("click", () => disable().catch(failed));
    node("#keep-size").addEventListener("change", () => saveSize().catch(failed));
  }
  const status = await refreshKeep();
  if (!(status.policy.enabled && status.permission)) return { keys: [] };
  const pulled = await send({ type: "keep.pull" });
  globalThis.graphshellInitialKeptJson = JSON.stringify({ policy: pulled.policy, pages: pulled.pages });
  return { keys: pulled.keys };
}

// The next delivery for a live batch, or null when none is waiting.
export async function pullKeep() {
  const status = await send({ type: "keep.status" });
  if (!(status.policy.enabled && status.permission) || status.queued === 0) return null;
  const pulled = await send({ type: "keep.pull" });
  return pulled.pages.length ? pulled : null;
}

// Acknowledge a delivery from the counts the app exposed on the root, or
// record its failure so the person sees why the pages are still waiting.
export async function finishKeep(keys, counts, prefix) {
  if (keys.length === 0) return { failed: false };
  const error = counts[`${prefix}Error`];
  if (error) {
    await send({ type: "keep.fail", error });
    await refreshKeep();
    return { failed: true, error };
  }
  await send({
    type: "keep.ack", keys,
    kept: Number(counts[`${prefix}Kept`] || 0),
    unchanged: Number(counts[`${prefix}Unchanged`] || 0),
    refused: Number(counts[`${prefix}Refused`] || 0),
  });
  await refreshKeep();
  return { failed: false };
}
