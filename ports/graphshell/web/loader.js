// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

const originalError = console.error.bind(console);

console.error = (...args) => {
  if (!document.title.startsWith("GRAPHSHELL H3 FAIL")) {
    document.title = `GRAPHSHELL H3 FAIL: ${args.map(String).join(" ").slice(0, 240)}`;
  }
  originalError(...args);
};

// Every page error, kept: the host rewrites the title each frame, so a title
// alone can hide one. A scenario receipt carries this list.
window.graphshellErrors = [];
window.addEventListener("error", (event) => {
  window.graphshellErrors.push(String(event.message));
  document.title = `GRAPHSHELL H3 FAIL: ${event.message}`;
});
window.addEventListener("unhandledrejection", (event) => {
  window.graphshellErrors.push(`unhandled rejection: ${event.reason}`);
});

function semanticNode(element) {
  const role =
    element.getAttribute("role") ||
    ({ BUTTON: "button", NAV: "navigation", MAIN: "main", SECTION: "region" }[
      element.tagName
    ] ?? null);
  const label =
    element.getAttribute("aria-label") ||
    (element.matches("button, h1, h2, dd") ? element.textContent.trim() : null);
  const children = [...element.children]
    .filter((child) => child.getAttribute("aria-hidden") !== "true")
    .map(semanticNode)
    .filter((child) => child.role || child.label || child.children.length);
  return {
    ...(role ? { role } : {}),
    ...(label ? { label } : {}),
    ...(element.id ? { id: element.id } : {}),
    ...(element.hasAttribute("aria-pressed")
      ? { pressed: element.getAttribute("aria-pressed") === "true" }
      : {}),
    children,
  };
}

// The component's root and its parts. Everything the receipt reads comes
// from under the root; the page's own ids are not consulted.
const graphshellRoot = () => document.querySelector("graphshell-view");
const part = (name) => graphshellRoot()?.querySelector(`#gs-${name}`);

window.graphshellSemanticTree = () => semanticNode(part("semantic-host"));

window.graphshellScenario = () => ({
  state: document.body.dataset.scenario ?? null,
  errors: [...window.graphshellErrors],
  result: JSON.parse(document.getElementById("scenario-result")?.textContent || "null"),
  captures: [...document.querySelectorAll("#scenario-captures img")].map((img) => ({
    name: img.dataset.capture,
    width: Number(img.width),
    height: Number(img.height),
    bytes: img.src.length,
  })),
});

window.graphshellReceipt = () => ({
  practice: graphshellRoot()?.hasAttribute('data-practice-workspace') ? {
    view: graphshellRoot().dataset.practiceView,
    selection: graphshellRoot().dataset.practiceSelection,
    history: Number(graphshellRoot().dataset.practiceHistory),
    layout: graphshellRoot().dataset.practiceLayout,
    motion: graphshellRoot().dataset.practiceMotion,
    settling: graphshellRoot().dataset.practiceSettling,
    metrics: JSON.parse(graphshellRoot().dataset.practiceMetrics || '{}'),
  } : null,
  title: document.title,
  ready: graphshellRoot().dataset.ready === "true",
  session: graphshellRoot().dataset.session,
  detailOpen: graphshellRoot().dataset.detailOpen === "true",
  actionCount: Number(graphshellRoot().dataset.actionCount || 0),
  storage: graphshellRoot().dataset.storage,
  capture: {
    accepted: Number(graphshellRoot().dataset.captureAccepted || 0),
    dropped: Number(graphshellRoot().dataset.captureDropped || 0),
  },
  remote: {
    link: graphshellRoot().dataset.remoteLink,
    state: graphshellRoot().dataset.remoteState,
    revision: graphshellRoot().dataset.remoteRevision,
    generation: graphshellRoot().dataset.remoteGeneration,
    cards: graphshellRoot().dataset.remoteCards,
    cardLabels: graphshellRoot().dataset.remoteCardLabels,
    overlaps: Number(graphshellRoot().dataset.remoteOverlaps || 0),
    resume: graphshellRoot().dataset.remoteResume,
    subject: graphshellRoot().dataset.remoteSubject,
    session: graphshellRoot().dataset.remoteSession,
    actions: [...graphshellRoot().querySelectorAll("#gs-remote-actions button")].map((b) => ({
      intent: b.dataset.intent,
      label: b.textContent,
    })),
  },
  chronicle: graphshellRoot()?.hasAttribute("data-chronicle-receipt") ? {
    receipt: graphshellRoot().dataset.chronicleReceipt,
    definition: graphshellRoot().dataset.chronicleDefinition,
    datasets: Number(graphshellRoot().dataset.chronicleDatasets || 0),
    shelfmark: graphshellRoot().dataset.chronicleShelfmark,
    variant: graphshellRoot().dataset.chronicleVariant,
    eraBands: graphshellRoot().dataset.chronicleEraBands,
    generation: graphshellRoot().dataset.chronicleGeneration,
  } : null,
  camera: part("graphshell-canvas").dataset.camera,
  focusedNode: part("graphshell-canvas").dataset.focusedNode,
  product: {
    status: graphshellRoot().dataset.productStatus,
    nodeCount: Number(graphshellRoot().dataset.nodeCount || 0),
    filterCount: Number(graphshellRoot().dataset.filterCount || 0),
    layout: graphshellRoot().dataset.layout,
    physicsPaused: graphshellRoot().dataset.physicsPaused === "true",
    selectedCount: Number(graphshellRoot().dataset.selectedCount || 0),
    exportBytes: Number(graphshellRoot().dataset.exportBytes || 0),
    importedNodes: Number(graphshellRoot().dataset.importedNodes || 0),
    relationFamily: graphshellRoot().dataset.relationFamily,
    face: graphshellRoot().dataset.face,
  },
  viewport: {
    width: window.innerWidth,
    height: window.innerHeight,
  },
  projectionEditor: {
    open: graphshellRoot().dataset.projectionEditorOpen === "true",
    panel: graphshellRoot().dataset.projectionEditorPanel,
    content: graphshellRoot().dataset.projectionEditorContent,
    validation: graphshellRoot().dataset.projectionEditorValidation,
    errors: Number(graphshellRoot().dataset.projectionEditorErrors || 0),
    saveCount: Number(graphshellRoot().dataset.projectionEditorSaveCount || 0),
    status: part("projection-editor-status")?.textContent,
    preview: part("projection-editor-preview")?.textContent,
    source: {
      authority: part("projection-source-authority")?.value,
      domain: part("projection-source-domain")?.value,
      resource: part("projection-source-resource")?.value,
    },
    reading: {
      key: part("projection-reading-key")?.value,
    },
    encoding: {
      x: part("projection-encoding-x")?.value,
      y: part("projection-encoding-y")?.value,
    },
    arrangement: {
      kind: part("projection-arrangement-kind")?.value,
      direction: part("projection-arrangement-direction")?.value,
      spacing: part("projection-arrangement-spacing")?.value,
    },
    appearance: {
      realization: part("projection-appearance-realization")?.value,
      title: part("projection-appearance-title")?.value,
    },
    provenance: {
      author: part("projection-provenance-author")?.value,
      sourceRevision: part("projection-provenance-revision")?.value,
      note: part("projection-provenance-note")?.value,
    },
  },
  semantics: window.graphshellSemanticTree(),
});

// A headed W2 run receives this compact, host-generated report in addition to
// the admitted scene. Graphshell checks the report's portable shape before
// exposing it to a scenario; it does not reconstruct Distillery or Djinn
// authority in browser JavaScript. The host remains responsible for the two
// actual reads and their authority-emitted generations.
const CHRONICLE_BINDING_RECEIPT_SCHEMA = "mere.chronicle-binding-receipt/1";
const SHELFMARK_V1_SCHEMA = "mere.shelfmark/1";

function isRecord(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function requiredText(value, name) {
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`Chronicle receipt ${name} must be non-empty text`);
  }
  return value;
}

function exactKeys(value, keys, name) {
  if (!isRecord(value)) {
    throw new Error(`Chronicle receipt ${name} must be an object`);
  }
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw new Error(`Chronicle receipt ${name} has an unsupported shape`);
  }
}

function chronicleBindingReceipt(value) {
  exactKeys(value, ["schema", "definition", "bindings", "variant", "shelfmark", "shelfmark_round_trip"], "root");
  if (value.schema !== CHRONICLE_BINDING_RECEIPT_SCHEMA) {
    throw new Error(`Chronicle receipt schema must be ${CHRONICLE_BINDING_RECEIPT_SCHEMA}`);
  }

  const definition = value.definition;
  exactKeys(definition, ["version", "id", "label", "sources", "reading", "encoding", "arrangement", "interaction", "appearance", "provenance"], "definition");
  const definitionId = requiredText(definition.id, "definition.id");
  exactKeys(definition.arrangement, ["kind", "direction", "spacing", "options"], "definition.arrangement");
  if (definition.arrangement.kind !== "timeline.default") {
    throw new Error("Chronicle receipt definition must use timeline.default");
  }
  if (!isRecord(definition.arrangement.options) || definition.arrangement.options.era_bands !== "true") {
    throw new Error("Chronicle receipt definition must keep era_bands enabled");
  }
  if (!isRecord(definition.sources)) {
    throw new Error("Chronicle receipt definition sources must be an object");
  }

  const variant = value.variant;
  exactKeys(variant, ["definition_id", "id", "arrangement_options"], "variant");
  if (variant.definition_id !== definitionId || variant.id !== "era-bands-off") {
    throw new Error("Chronicle receipt variant must be era-bands-off for this definition");
  }
  exactKeys(variant.arrangement_options, ["era_bands"], "variant.arrangement_options");
  if (variant.arrangement_options.era_bands !== "false") {
    throw new Error("Chronicle receipt era-bands-off variant must disable era_bands");
  }

  if (!Array.isArray(value.bindings) || value.bindings.length !== 2) {
    throw new Error("Chronicle receipt must carry exactly two dataset bindings");
  }
  const roles = new Set();
  for (const [index, binding] of value.bindings.entries()) {
    exactKeys(binding, ["role", "definition_id", "source", "generation", "read"], `bindings[${index}]`);
    const role = requiredText(binding.role, `bindings[${index}].role`);
    if (roles.has(role)) {
      throw new Error(`Chronicle receipt repeats binding role ${role}`);
    }
    roles.add(role);
    if (binding.definition_id !== definitionId || binding.read !== true) {
      throw new Error(`Chronicle receipt binding ${role} is not a successful read of ${definitionId}`);
    }
    exactKeys(binding.source, ["authority", "domain", "resource"], `bindings[${index}].source`);
    requiredText(binding.source.authority, `bindings[${index}].source.authority`);
    requiredText(binding.source.domain, `bindings[${index}].source.domain`);
    requiredText(binding.source.resource, `bindings[${index}].source.resource`);
    requiredText(binding.generation, `bindings[${index}].generation`);
    const authored = definition.sources[binding.role];
    exactKeys(authored, ["source", "expects_generation"], `definition.sources.${role}`);
    for (const key of ["authority", "domain", "resource"]) {
      if (authored.source[key] !== binding.source[key]) {
        throw new Error(`Chronicle receipt authored source does not match ${role}`);
      }
    }
    if (authored.expects_generation !== binding.generation) {
      throw new Error(`Chronicle receipt authored generation does not match ${role}`);
    }
  }
  const authoredRoles = Object.keys(definition.sources).sort();
  const boundRoles = [...roles].sort();
  if (authoredRoles.length !== boundRoles.length || authoredRoles.some((role, index) => role !== boundRoles[index])) {
    throw new Error("Chronicle receipt definition sources do not match the two reads");
  }

  const shelfmark = value.shelfmark;
  exactKeys(shelfmark, ["schema", "projection", "inputs", "delta"], "shelfmark");
  if (shelfmark.schema !== SHELFMARK_V1_SCHEMA || shelfmark.projection !== definitionId) {
    throw new Error("Chronicle receipt shelfmark does not cite the authored definition");
  }
  if (!isRecord(shelfmark.inputs)) {
    throw new Error("Chronicle receipt shelfmark inputs must be an object");
  }
  const shelfmarkRoles = Object.keys(shelfmark.inputs).sort();
  if (shelfmarkRoles.length !== boundRoles.length || shelfmarkRoles.some((role, index) => role !== boundRoles[index])) {
    throw new Error("Chronicle receipt shelfmark inputs do not match the two reads");
  }
  for (const binding of value.bindings) {
    const input = shelfmark.inputs[binding.role];
    exactKeys(input, ["authority", "reading", "expects_generation"], `shelfmark.inputs.${binding.role}`);
    exactKeys(input.authority, ["adapter", "record"], `shelfmark.inputs.${binding.role}.authority`);
    if (input.authority.adapter !== "distillery.chronicle/v1" || input.reading !== "chronicle") {
      throw new Error(`Chronicle receipt shelfmark adapter does not match ${binding.role}`);
    }
    let authorityRecord;
    try {
      authorityRecord = JSON.parse(input.authority.record);
    } catch (_error) {
      throw new Error(`Chronicle receipt shelfmark authority record is invalid for ${binding.role}`);
    }
    for (const key of ["authority", "domain", "resource"]) {
      if (authorityRecord[key] !== binding.source[key]) {
        throw new Error(`Chronicle receipt shelfmark authority does not match ${binding.role}`);
      }
    }
    if (input.expects_generation !== binding.generation) {
      throw new Error(`Chronicle receipt shelfmark generation does not match ${binding.role}`);
    }
  }
  if (value.shelfmark_round_trip !== true) {
    throw new Error("Chronicle receipt shelfmark did not round-trip");
  }

  const djinn = value.bindings.find((binding) => binding.role === "djinn");
  if (!djinn) {
    throw new Error("Chronicle receipt must include the Djinn resident read");
  }
  return { definitionId, datasetCount: value.bindings.length, djinnGeneration: djinn.generation };
}

async function waitForMountedGeneration() {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    const root = graphshellRoot();
    if (root.dataset.remoteState === "failed") {
      throw new Error("Chronicle receipt remote mount failed");
    }
    if (root.dataset.remoteState === "open" && root.dataset.remoteGeneration) {
      return root.dataset.remoteGeneration;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error("Chronicle receipt remote generation was not mounted");
}

async function loadChronicleBindingReceipt(params) {
  const location = params.get("chronicle-receipt");
  if (!location) return;
  const response = await fetch(new URL(location, window.location.href), { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Chronicle receipt ${location}: HTTP ${response.status}`);
  }
  let report;
  try {
    report = chronicleBindingReceipt(await response.json());
  } catch (error) {
    throw new Error(`Chronicle receipt ${location}: ${error.message}`);
  }
  const root = graphshellRoot();
  if (params.get("signal")) {
    const mountedGeneration = await waitForMountedGeneration();
    if (mountedGeneration !== report.djinnGeneration) {
      throw new Error(`Chronicle receipt Djinn generation ${report.djinnGeneration} does not match mounted generation ${mountedGeneration}`);
    }
  }
  root.dataset.chronicleReceipt = "verified";
  root.dataset.chronicleDefinition = report.definitionId;
  root.dataset.chronicleDatasets = String(report.datasetCount);
  root.dataset.chronicleShelfmark = "round-trip";
  root.dataset.chronicleVariant = "era-bands-off";
  root.dataset.chronicleEraBands = "false";
  root.dataset.chronicleGeneration = report.djinnGeneration;
}

try {
  if ((globalThis.browser ?? globalThis.chrome)?.runtime?.id) {
    await import("./capture-model.js");
    const extensionProfile = await import("./extension-profile.js");
    await extensionProfile.prepareCapture();
  }
  const module = await import("./pkg/graphshell_web.js");
  await module.default();
  // Two ways in, one component. `mountGraphshell(element)` is the plain
  // entry; `<graphshell-view>` is the same entry as a custom element, mounted
  // when it connects. Elements already in the document upgrade on define.
  window.mountGraphshell = (element) => module.mount(element);
  if (!customElements.get("graphshell-view")) {
    customElements.define(
      "graphshell-view",
      class extends HTMLElement {
        connectedCallback() {
          if (!this.dataset.mounted) {
            this.dataset.mounted = "true";
            module.mount(this);
          }
        }
      },
    );
  }
  // The remote link: `?signal=<url>` joins a host over WebRTC through its
  // signaling server (`GET /invite` unless `?invite=` is given, `POST
  // /offer`). Without it the in-process fixture stays mounted.
  const params = new URLSearchParams(location.search);
  const signal = params.get("signal");
  if (signal) {
    await new Promise((resolve) => {
      const poll = () =>
        graphshellRoot().dataset.ready === "true" ? resolve() : setTimeout(poll, 50);
      poll();
    });
    module.connect_remote(signal, params.get("invite"));
  }
  // W2 binds the mounted scene to a host-generated report. It is loaded before
  // the page-side scenario, so its verified fields and the remote carrier are
  // recorded together in the same headed receipt.
  await loadChronicleBindingReceipt(params);
  // The scenario lane: `?scenario=<path>` names a script the page runs on
  // itself once the host reports ready. Results land in the DOM (see
  // src/web_scenario.rs); nothing here interprets them.
  const scenarioPath = params.get("scenario");
  if (scenarioPath) {
    // Never from the HTTP cache: a receipt profile persists across runs, and
    // a scenario edited between them must be the one that runs.
    const response = await fetch(scenarioPath, { cache: "no-store" });
    if (!response.ok) {
      throw new Error(`scenario ${scenarioPath}: HTTP ${response.status}`);
    }
    const text = await response.text();
    await new Promise((resolve) => {
      const poll = () =>
        graphshellRoot().dataset.ready === "true" ? resolve() : setTimeout(poll, 50);
      poll();
    });
    // `?sink=<url>` names a receipt sink: when the run completes, the result,
    // the host receipt and every capture (as data URLs) are POSTed there as
    // one JSON body, so a driver outside the page collects files rather
    // than reading a DOM it may not be able to reach.
    const sink = new URLSearchParams(location.search).get("sink");
    if (sink) {
      // Progress while the run is alive, so a run that never completes
      // still says how far it got: the same state the DOM shows, posted
      // every two seconds to the sink's `/scenario-progress`.
      const progress = sink.replace(/\/scenario-receipt$/, "/scenario-progress");
      const beat = setInterval(async () => {
        if (document.body.dataset.scenario !== "running") {
          clearInterval(beat);
          return;
        }
        try {
          await fetch(progress, {
            method: "POST",
            headers: { "content-type": "application/json" },
            body: JSON.stringify({
              scenario: window.graphshellScenario(),
              frames: document.body.dataset.scenarioFrames,
              log: document.getElementById("scenario-log")?.textContent,
              remote: window.graphshellReceipt().remote,
              actionStatus: part("action-status")?.textContent,
            }),
          });
        } catch (_) {
          // The sink may be gone; the receipt is what matters.
        }
      }, 2000);
      document.addEventListener(
        "graphshell-scenario-complete",
        async () => {
          const scenario = window.graphshellScenario();
          const captures = [...document.querySelectorAll("#scenario-captures img")].map(
            (img) => ({ name: img.dataset.capture, dataUrl: img.src }),
          );
          try {
            await fetch(sink, {
              method: "POST",
              headers: { "content-type": "application/json" },
              body: JSON.stringify({
                scenario,
                receipt: window.graphshellReceipt(),
                captures,
              }),
            });
            document.body.dataset.scenarioSink = "delivered";
          } catch (error) {
            document.body.dataset.scenarioSink = `failed: ${error}`;
          }
        },
        { once: true },
      );
    }
    module.run_scenario(text);
  }
} catch (error) {
  document.title = `GRAPHSHELL H3 FAIL: ${error}`;
  originalError(error);
}
