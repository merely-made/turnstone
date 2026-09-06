// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
// Proof host only. Rust Commons/Gemot owns acceptance, authority, and persistence.
const $ = (id) => document.getElementById(id);
let latest;
let painted = "";

function tones(values) { return (values ?? []).map((tone) => tone.label).join(" · "); }
function recordedComparison(record) {
  try {
    const hex = record.body_utf8_hex;
    if (typeof hex !== "string" || !/^(?:[0-9a-f]{2})*$/i.test(hex)) return null;
    const bytes = Uint8Array.from(hex.match(/../g) ?? [], (pair) => Number.parseInt(pair, 16));
    const value = JSON.parse(new TextDecoder("utf-8", {fatal: true}).decode(bytes));
    return value.method?.id === "keyed-pitch-set-singleton" && value.method.version === 1 && value.result?.version === 1 ? value : null;
  } catch { return null; }
}
function recordLabel(record) {
  const comparison = recordedComparison(record);
  if (!comparison) return "Unrecognized shared comparison";
  const motion = comparison.result.singleton_motion;
  return `Shared ${tones(comparison.result.shared)}${motion ? ` / ${motion.from.label} to ${motion.to.label}` : ""}`;
}
function disclose(state) {
  const comparison = state.comparison;
  const cards = [comparison.left, comparison.right];
  const occurrences = cards.map((card, index) => ({
    occurrence_id: card.occurrence_id,
    source: card.source,
    values: {
      occurrence_id: {kind: "text", value: card.occurrence_id},
      label: {kind: "text", value: card.label},
      order: {kind: "number", value: index + 1},
      row: {kind: "number", value: 0},
    },
  }));
  // These rows come exclusively from the service's authority-filtered projection.
  for (const [index, record] of state.records.entries()) {
    occurrences.push({
      occurrence_id: `comparison:${record.id}`,
      source: {adapter: "commons.practice", id: record.id},
      values: {
        occurrence_id: {kind: "text", value: `comparison:${record.id}`},
        label: {kind: "text", value: recordLabel(record)},
        order: {kind: "number", value: index + 1},
        row: {kind: "number", value: 1},
      },
    });
  }
  return {
    source: comparison.source,
    revision: `${comparison.revision}:${state.records.map((r) => r.id).sort().join(",")}`,
    fields: {occurrence_id: "text", label: "text", order: "number", row: "number"},
    occurrences,
  };
}

function paint() {
  if (!latest) return;
  const root = $("graphshell");
  if (root.dataset.ready !== "true") return;
  const dataset = disclose(latest);
  const json = JSON.stringify(dataset);
  if (json === painted) return;
  root.setAttribute("data-projection-dataset", json);
  root.setAttribute("data-projection-definition", JSON.stringify({
    version: 1, id: "shared-practice", label: "Thursday practice",
    source: dataset.source,
    reading: {kind: "nodes", key: "occurrence_id", value: null},
    encoding: {x: {kind: "field", value: "order"}, y: {kind: "field", value: "row"}, color: null, label: {kind: "field", value: "label"}},
    arrangement: {kind: "grid.default", direction: "coordinates", spacing: 16},
    interaction: {selection: "single", pan: false, zoom: false},
    appearance: {realization: "canvas", title: "Thursday practice", theme: "slate"},
    provenance: {author: "Graphshell shared practice proof", source_revision: dataset.revision, note: "Rows separate source material and authority-filtered contributions."},
  }));
  root.dispatchEvent(new Event("graphshell-project-dataset"));
  painted = json;
}
new MutationObserver(paint).observe($("graphshell"), {attributes: true, attributeFilter: ["data-ready"]});

function render(state) {
  latest = state;
  $("peer-name").textContent = `${state.role === "founder" ? "Alex · founder" : "Bea · invited member"} · independent local store`;
  $("connection-status").textContent = `${state.online ? "Peer exchange enabled" : "Offline"} · ${state.process_open ? "store open" : "store closed"}${state.last_sync_error ? " · last sync failed" : ""}`;
  $("found").hidden = state.role !== "founder";
  $("join").hidden = state.role === "founder";
  $("shared-tones").textContent = tones(state.comparison.result.shared);
  const motion = state.comparison.result.singleton_motion;
  $("moved-tones").textContent = motion ? `${motion.from.label} → ${motion.to.label}` : "No singleton comparison";
  $("comparison-kind").textContent = "Local catalog preview. Shared contributions appear below and on the canvas.";
  $("retained-status").textContent = `${state.records.length} effective comparison(s) · ${state.retained_count} retained operation(s)`;
  $("shared-records").replaceChildren(...state.records.map((record) => {
    const li = document.createElement("li");
    li.textContent = `${recordLabel(record)} · ${record.id.slice(0, 14)}`;
    li.dataset.recordId = record.id;
    return li;
  }));
  $("evidence").textContent = JSON.stringify({service: state.service, traffic: state.traffic}, null, 2);
  document.body.dataset.peerRole = state.role;
  document.body.dataset.sharedComparisons = String(state.records.length);
  document.body.dataset.online = String(state.online);
  document.body.dataset.storeOpen = String(state.process_open);
  paint();
}

async function refresh() {
  const response = await fetch("/api/state", {cache: "no-store"});
  const state = await response.json();
  if (!response.ok) throw new Error(state.error ?? "Could not read local store");
  render(state);
}

for (const button of document.querySelectorAll("[data-action]")) {
  button.addEventListener("click", async () => {
    const buttons = [...document.querySelectorAll("[data-action]")];
    buttons.forEach((b) => { b.disabled = true; });
    $("action-result").textContent = "Working…";
    $("action-result").dataset.error = "false";
    try {
      const response = await fetch("/api/action", {method: "POST", headers: {"content-type": "application/json"}, body: JSON.stringify({action: button.dataset.action})});
      const result = await response.json();
      if (!response.ok) throw new Error(result.error ?? "Action failed");
      render(result.state);
      $("action-result").textContent = result.message;
      $("action-result").dataset.error = String(Boolean(result.state.last_sync_error));
    } catch (error) {
      $("action-result").textContent = error.message;
      $("action-result").dataset.error = "true";
    } finally { buttons.forEach((b) => { b.disabled = false; }); }
  });
}
refresh().catch((error) => { $("action-result").textContent = error.message; });
// Read-only status refresh makes incoming peer contributions visible.
setInterval(() => refresh().catch(() => {}), 2500);
