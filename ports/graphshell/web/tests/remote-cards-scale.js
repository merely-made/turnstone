// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
// Build graphshell-web --release --features remote-cards-receipt, bindgen into
// web/pkg, then serve web/ with the scenario receipt sink. Run this page with
// ?cards=1000 and ?cards=5000 in separate windows of the same size.

import { defineElements } from "../mount.js";
import init, * as graphshell from "../pkg/graphshell_web.js";
import { quantiles } from "./remote-cards-scale-stats.js";

const params = new URLSearchParams(location.search);
const size = Number(params.get("cards") ?? 1000);
const repeats = Number(params.get("edits") ?? 200);
const warmup = Number(params.get("warmup") ?? 12);
const compact = params.get("labels") === "compact";
const mode = params.get("mode") ?? "optimized";
const layoutPath = params.get("layout");
const visibleOnly = params.get("phase") === "visible";
const geometryOnly = params.get("phase") === "geometry";
const geometryCase = params.get("geometry_case") ?? "both";
const settleMs = Number(params.get("settle_ms") ?? 0);
const requireGeometryPublication = params.get("geometry_work") === "required";
const requireLocalWork = params.get("local_work") === "required";
const requireGeometryCull = params.get("geometry_cull") === "required";
const traceGeometry = params.get("trace_geometry") === "true";
const measurePublication = params.has("publication_work");
const requirePublication = params.get("publication_work") === "required";
const root = document.querySelector("#graphshell");
const tick = () => new Promise(requestAnimationFrame);
const read = ({ sample }) => sample.rows;
let lastAttempt;
let geometryReads;
let publicationObserver;
let mirrorRoot;
let mirrorRecords = [];

if (traceGeometry) {
  const record = (element, property, read) => {
    if (!geometryReads || !(element instanceof Element) ||
        !(element instanceof HTMLCanvasElement || element.hasAttribute("data-cambium-mirror"))) {
      return read();
    }
    const start = performance.now();
    const result = read();
    geometryReads.push({ target: element.tagName, property, elapsed_ms: performance.now() - start });
    return result;
  };
  const rect = Element.prototype.getBoundingClientRect;
  Element.prototype.getBoundingClientRect = function () {
    return record(this, "getBoundingClientRect", () => rect.call(this));
  };
  for (const property of ["offsetLeft", "offsetTop"]) {
    const descriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, property);
    Object.defineProperty(HTMLElement.prototype, property, {
      ...descriptor,
      get() { return record(this, property, () => descriptor.get.call(this)); },
    });
  }
}
const report = {
  size, warmup, repeats, compact, mode, viewport: [innerWidth, innerHeight], dpr: devicePixelRatio,
  browser: navigator.userAgent,
};

function require(condition, message) {
  if (!condition) throw new Error(message);
}

async function frame(index, last = false, panTo) {
  if (traceGeometry) geometryReads = [];
  if (publicationObserver) {
    publicationObserver.takeRecords();
    mirrorRecords = [];
  }
  graphshell.cards_receipt_request(index, last, panTo);
  for (let i = 0; i < 600; i++) {
    await tick();
    const result = graphshell.cards_receipt_take();
    if (result) {
      lastAttempt = JSON.parse(result);
      if (traceGeometry) {
        lastAttempt.geometry_reads = geometryReads;
        geometryReads = undefined;
      }
      if (publicationObserver) lastAttempt.publication_work = publicationWork();
      return lastAttempt;
    }
  }
  throw new Error("Graphshell did not accept a frame containing Cards");
}

function compare(before, after, changed) {
  require(after.length === 12 && before.length === 12, "expected 12 mounted cards");
  const labels = [];
  for (let i = 0; i < before.length; i++) {
    require(before[i][0] === after[i][0] && before[i][1] === after[i][1], `row ${i} was replaced`);
    if (before[i][2] !== after[i][2]) labels.push(after[i][0] - 1);
  }
  require(JSON.stringify(labels) === JSON.stringify(changed), `unexpected label changes ${labels}`);
}

function publicationWork() {
  for (const record of publicationObserver.takeRecords()) mirrorRecords.push(record);
  const board = mirrorRoot.querySelector('[role="list"][aria-label^="Remote board"]');
  const items = new Set();
  let boardMutations = 0;
  for (const record of mirrorRecords) {
    if (board?.contains(record.target)) {
      boardMutations++;
      const element = record.target instanceof Element ? record.target : record.target.parentElement;
      const item = element?.closest('[role="listitem"]');
      if (item) items.add(item);
    }
  }
  return {
    mirror_mutations: mirrorRecords.length,
    board_mutations: boardMutations,
    board_items_changed: items.size,
    proxy_replacements: mirrorRecords.filter(record => record.type === "childList").length,
    mirror_style_writes: mirrorRecords.filter(record => record.target === mirrorRoot && record.attributeName === "style").length,
  };
}

function metrics({ elapsed_ms, ingress_ms, sample, frame: profile, geometry_reads, publication_work }) {
  require(profile !== null, "accepted frame has no Rootstock profile");
  const { rows, presentation, revision, panned_title_glyphs, ...work } = sample;
  const measured = { elapsed_ms, ingress_ms, ...work, ...profile };
  if (publication_work) {
    for (const [name, value] of Object.entries(publication_work)) measured[`a11y_${name}`] = value;
  }
  if (geometry_reads) {
    measured.geometry_read_ms = geometry_reads.reduce((total, read) => total + read.elapsed_ms, 0);
    measured.mirror_geometry_reads = geometry_reads.filter(read => read.target === "DIV").length;
  }
  return measured;
}

function mirrorContains(label, listName = "Cards") {
  return mirrorItem(label, listName) !== undefined;
}

function mirrorItem(label, listName) {
  const list = document.querySelector(`[data-cambium-mirror] [role="list"][aria-label^="${listName}"]`);
  return [...(list?.querySelectorAll('[role="listitem"]') ?? [])].find(item =>
    (item.getAttribute("aria-label") ?? item.textContent).trim() === label);
}

function label(index, changed) {
  if (compact) {
    const prefix = String.fromCharCode((changed ? 65 : 48) + Math.floor(index / 1000));
    return `${prefix}${String(index % 1000).padStart(3, "0")}`;
  }
  return `Card ${String(index).padStart(5, "0")} ${changed ? "B" : "A"}`;
}

function progress(phase, samples) {
  return fetch("/scenario-progress", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ size, phase, completed: samples.length, metrics: quantiles(samples) }),
  });
}

async function run() {
  await init();
  if (layoutPath === "full") root.setAttribute("data-force-full-layout", "");
  if (layoutPath !== null) report.layout_path = layoutPath;
  defineElements(graphshell);
  for (let i = 0; i < 600 && root.dataset.ready !== "true"; i++) {
    require(!root.hasAttribute("data-mount-error"), root.getAttribute("data-mount-error"));
    await tick();
  }
  require(root.dataset.ready === "true", "Graphshell did not mount");
  if (measurePublication) {
    mirrorRoot = document.querySelector('[data-cambium-mirror]');
    require(mirrorRoot, "accessibility mirror missing");
    publicationObserver = new MutationObserver(records => {
      for (const record of records) mirrorRecords.push(record);
    });
    publicationObserver.observe(mirrorRoot, { subtree: true, attributes: true, childList: true, characterData: true });
  }
  report.backend = root.dataset.selectedRenderBackend;
  const requested = params.get("render_backend");
  if (requested) require(report.backend === requested, `expected ${requested}, mounted ${report.backend}`);
  graphshell.cards_receipt_mount(size, compact, mode);
  await tick();
  let previous = await frame();
  report.initial = previous;
  report.initial_label_in_mirror = mirrorContains(label(5, false));
  require(report.initial_label_in_mirror, "initial Cards label missing from accessibility mirror");
  if (measurePublication) {
    const board = mirrorRoot.querySelector('[role="list"][aria-label^="Remote board"]');
    report.initial_board_items = board?.querySelectorAll('[role="listitem"]').length;
    require(report.initial_board_items === size, "initial accessibility board is incomplete");
  }
  require(read(previous).some(row => row[0] === 6 && row[3]), "edited row is clipped");
  if (geometryOnly) {
    await geometryRun(previous);
    return;
  }
  const beforePan = await frame(undefined, false, size - 1);
  report.pan_before_edit = beforePan;
  require(beforePan.sample.title_runs > 0, "pan did not paint any titles");
  previous = await frame();
  require(previous.sample.title_runs === 12, `fixed camera painted ${previous.sample.title_runs} title runs`);
  for (let i = 0; i < warmup; i++) {
    previous = await frame(5);
  }
  const cases = [["visible", 5, [5]]];
  if (!visibleOnly) cases.push(["offscreen", size - 1, []]);
  for (const [name, index, changes] of cases) {
    const offscreenBoardItem = name === "offscreen" && mirrorItem(label(index, false), "Remote board");
    const visibleBoardItem = name === "visible" && requireLocalWork
      ? mirrorItem(read(previous).find(row => row[0] === 6)[2], "Remote board")
      : undefined;
    if (mode === "optimized" && name === "offscreen") {
      const idle = await frame();
      require(idle.sample.canvas_reused && idle.sample.canvas_revision === previous.sample.canvas_revision,
        "an idle frame changed the canvas revision");
      report.idle_proof = idle;
      previous = idle;
    }
    const samples = [];
    if (name === "visible") report.visible_started_ms = Date.now();
    for (let i = 0; i < repeats; i++) {
      const current = await frame(index);
      compare(read(previous), read(current), changes);
      require(current.sample.title_runs === 12, `fixed camera painted ${current.sample.title_runs} title runs`);
      if (requireGeometryCull) {
        require(current.sample.board_rects === 25, `fixed camera emitted ${current.sample.board_rects} rectangles`);
      }
      require(current.sample.rows_built === changes.length, `${name} edit built ${current.sample.rows_built} row views`);
      require(current.sample.revision === previous.sample.revision + 1, "revision did not advance once");
      require(current.sample.presentation > previous.sample.presentation, "frame was not accepted");
      require(Number(root.dataset.remoteRevision) === current.sample.revision, "diagnostic revision did not advance");
      if (i === 0) {
        report[`${name}_proof`] = current;
        if (name === "visible") {
          const edited = read(current).find(row => row[0] === 6)[2];
          report.changed_label_in_mirror = mirrorContains(edited);
          require(report.changed_label_in_mirror, "edited Cards label missing from accessibility mirror");
          if (requireLocalWork) {
            const item = mirrorItem(edited, "Remote board");
            require(item && item === visibleBoardItem, "visible board name did not update in place");
            report.visible_board_name_patched = true;
          }
        }
        if (name === "offscreen") {
          const edited = mirrorItem(label(index, true), "Remote board");
          require(edited && edited === offscreenBoardItem, "offscreen board name did not update in place");
          report.offscreen_board_name_patched = true;
        }
      }
      if (name === "offscreen") {
        const legacy = mode === "legacy";
        require(current.frame.mutations === (legacy ? 2 : 0), `offscreen edit produced ${current.frame.mutations} DOM mutations`);
        require(current.frame.layout_rebuilt === legacy, "unexpected layout rebuild for offscreen edit");
      }
      if (name === "visible" && layoutPath !== null) {
        require(current.frame.layout_retained === (layoutPath === "retained"),
          `visible edit used the wrong layout path for ${layoutPath}`);
      }
      if (name === "visible" && requireLocalWork) {
        require(current.sample.semantic_cards === 0, "visible edit rebuilt board semantics");
        require(current.frame.restyled_elements === 0, `visible label restyled ${current.frame.restyled_elements} elements`);
      }
      if (requirePublication) {
        const work = current.publication_work;
        require(work.board_items_changed === 1 && work.board_mutations === 1,
          `${name} edit changed ${work.board_items_changed} board items with ${work.board_mutations} mutations`);
        require(work.proxy_replacements === 0, "label edit replaced accessibility proxies");
        require(work.mirror_style_writes === 0, "label edit rewrote mirror positioning styles");
        if (traceGeometry) require(current.geometry_reads.filter(read => read.target === "DIV").length === 3,
          "publication aligned the mirror more than once");
      }
      if (mode === "optimized") {
        const sample = current.sample;
        require(sample.labels_patched === 1 && sample.labels_projected === 0, "expected one indexed Cards label patch");
        require(sample.action_projections === 0, "label edit reprojected advertised actions");
        require(sample.board_label_patches === 1, "label edit did not patch the board");
        const offscreen = name === "offscreen";
        require(offscreen ? sample.canvas_reused && sample.board_paint_ms === 0 : sample.board_paint_ms > 0,
          `unexpected board paint for ${name} edit`);
        require((sample.canvas_revision === previous.sample.canvas_revision) === offscreen, `unexpected canvas revision for ${name} edit`);
        if (offscreen) {
          require(sample.semantic_cards === 0, "offscreen edit rebuilt board semantics");
          if (report.backend === "webgpu") {
            require(current.frame.portable_materialization_reused, "offscreen edit materialized the portable scene again");
          }
        }
      }
      samples.push(metrics(current));
      previous = current;
      if (samples.length % 25 === 0) await progress(name, samples);
    }
    if (name === "visible") report.visible_finished_ms = Date.now();
    report[name] = quantiles(samples);
  }
  if (measurePublication) {
    const board = mirrorRoot.querySelector('[role="list"][aria-label^="Remote board"]');
    report.published_board_items = board?.querySelectorAll('[role="listitem"]').length;
    require(report.published_board_items === size, "label edits dropped accessible board items");
  }
  if (visibleOnly) return;
  // Ensure the revealed offscreen value differs from the initial A label.
  if (repeats % 2 === 0) await frame(size - 1);
  const last = await frame(undefined, true, size - 1);
  report.revealed = last;
  const expected = label(size - 1, true);
  require(read(last).at(-1)[2] === expected, "offscreen edit missing after End");
  report.revealed_label_in_mirror = mirrorContains(expected);
  report.revealed_board_label_in_mirror = mirrorContains(expected, "Remote board");
  require(report.revealed_label_in_mirror, "revealed Cards label missing from accessibility mirror");
  require(report.revealed_board_label_in_mirror, "revealed board label missing from accessibility mirror");
  if (mode === "optimized") {
    require(last.sample.semantic_cards === size, "pan did not refresh board geometry in the mirror");
    if (report.backend === "webgpu") {
      require(!last.frame.portable_materialization_reused, "pan reused the prior materialized scene");
    }
  }
  require(last.sample.title_runs === beforePan.sample.title_runs, "pan changed the set of title runs");
  if (requireGeometryCull) {
    require(last.sample.board_rects === beforePan.sample.board_rects, "pan changed the set of card rectangles");
    require(last.sample.board_rects > 1 && last.sample.board_rects < 200, "panned paint did not cull distant cards");
  }
  const beforeGlyphs = beforePan.sample.panned_title_glyphs.map(run => JSON.stringify(run));
  const afterGlyphs = last.sample.panned_title_glyphs.map(run => JSON.stringify(run));
  require(afterGlyphs.filter((run, index) => run !== beforeGlyphs[index]).length === 1, "expected one panned title's glyphs to change");
  if (requirePublication) {
    const canvas = root.querySelector("canvas");
    const aligned = () => {
      const painted = canvas.getBoundingClientRect();
      const mirrored = mirrorRoot.getBoundingClientRect();
      for (const key of ["left", "top", "width", "height"]) {
        require(Math.abs(painted[key] - mirrored[key]) <= 0.5, `mirror ${key} does not match canvas`);
      }
    };
    aligned();
    const transform = canvas.style.transform;
    canvas.style.transform = "translate(17px, 23px)";
    await frame();
    aligned();
    canvas.style.transform = transform;
    const width = root.style.width;
    root.style.width = "1000px";
    await tick();
    await tick();
    await frame();
    aligned();
    root.style.width = width;
    await tick();
    await tick();
    await frame();
    aligned();
    report.mirror_alignment_after_move_and_resize = true;
    const button = [...mirrorRoot.querySelectorAll('[role="button"]')].find(element =>
      element.getAttribute("aria-label") === "Graph commands" || element.textContent.trim().startsWith("Graph commands"));
    require(button, "Graph commands accessibility button missing");
    button.focus({ preventScroll: true });
    await frame(5);
    require(document.activeElement === button, "label edit moved accessibility focus");
    button.click();
    await tick();
    await tick();
    await frame();
    require(mirrorRoot.textContent.includes("Search graph commands") ||
      [...mirrorRoot.querySelectorAll('[aria-label]')].some(element => element.getAttribute("aria-label") === "Search graph commands"),
      "accessibility action did not open Graph commands");
    report.focus_and_action_after_label_patch = true;
  }
}

// The geometry workload changes the camera or surface, never the scene data.
// DOM/geometry correctness probes run outside the measured intervals.
async function geometryRun(initial) {
  require(measurePublication, "geometry fixture requires publication_work=measure|required");
  require(["pan", "resize", "both"].includes(geometryCase), "unknown geometry_case");
  require(Number.isFinite(settleMs) && settleMs >= 0, "settle_ms must be a finite nonnegative duration");
  if (requireGeometryPublication) require(typeof graphshell.cards_receipt_geometry_request === "function" &&
    typeof graphshell.cards_receipt_geometry_take === "function", "candidate geometry oracle exports are missing");
  report.phase = "geometry";
  report.geometry_case = geometryCase;
  report.settle_ms = settleMs;
  report.startup_max_background_pct = Number(params.get("startup_max_background") ?? 32);
  report.trace_geometry = traceGeometry;
  const canvas = root.querySelector("canvas");
  const board = () => mirrorRoot.querySelector('[role="list"][aria-label^="Remote board"]');
  const initialItems = [...board().querySelectorAll('[role="listitem"]')];
  const originalNames = initialItems.map(item => (item.getAttribute("aria-label") ?? item.textContent).trim());
  require(originalNames.every((name, index) => name === label(index, false)), "initial accessible board labels/order differ from the scene");
  const probes = [0, 5, Math.floor(size / 2), size - 1].map(index => initialItems[index]);
  const button = [...mirrorRoot.querySelectorAll('[role="button"]')].find(element =>
    element.getAttribute("aria-label") === "Graph commands" || element.textContent.trim().startsWith("Graph commands"));
  require(button, "Graph commands accessibility button missing");
  button.focus({ preventScroll: true });
  let previous = await frame();
  let currentPan;
  require(document.activeElement === button, "initial accessibility focus was not retained");
  const assertItems = () => {
    const items = [...board().querySelectorAll('[role="listitem"]')];
    require(items.length === size, "geometry publication dropped accessible board items");
    require(items.every((item, index) => item === initialItems[index]), "geometry publication replaced or reordered board items");
    require(items.every((item, index) => (item.getAttribute("aria-label") ?? item.textContent).trim() === originalNames[index]),
      "geometry publication changed accessible board names");
    require(probes.every(item => item.isConnected), "geometry publication detached sampled board items");
    return items;
  };
  const aligned = () => {
    const painted = canvas.getBoundingClientRect();
    const mirrored = mirrorRoot.getBoundingClientRect();
    for (const key of ["left", "top", "width", "height"]) {
      require(Math.abs(painted[key] - mirrored[key]) <= 0.5, `mirror ${key} does not match canvas`);
    }
  };
  const geometryProof = async (name, { expectedSize = size, retainOriginal = true } = {}) => {
    aligned();
    const items = retainOriginal ? assertItems() : [...board().querySelectorAll('[role="listitem"]')];
    require(items.length === expectedSize, `${name} published the wrong card count`);
    if (!retainOriginal) require(items.every((item, index) =>
      (item.getAttribute("aria-label") ?? item.textContent).trim() === label(index, false)),
      `${name} published names/order that differ from the remounted scene`);
    const mirrorRect = mirrorRoot.getBoundingClientRect();
    const proof = { items: items.length, focused: document.activeElement === button,
      canvas_css_size: [mirrorRect.width, mirrorRect.height], bounds: [] };
    // Candidate-only oracle recomputes sampled rectangles from the authoritative
    // scene after timing has stopped. Existing baseline builds omit this export.
    if (typeof graphshell.cards_receipt_geometry_request === "function") {
      graphshell.cards_receipt_geometry_request();
      await frame(undefined, false, currentPan);
      const encoded = graphshell.cards_receipt_geometry_take();
      require(encoded, "geometry oracle did not publish a receipt");
      const reference = JSON.parse(encoded);
      for (const expected of reference.samples) {
        const item = items.find(item => (item.getAttribute("aria-label") ?? item.textContent).trim() === expected.label);
        require(item, `geometry oracle item missing: ${expected.label}`);
        const actual = item.getBoundingClientRect();
        const rect = [actual.left - mirrorRect.left, actual.top - mirrorRect.top, actual.width, actual.height];
        require(rect.every((value, index) => Math.abs(value - expected.rect[index]) <= 0.75),
          `${name} accessible bounds differ for ${expected.label}: ${JSON.stringify(rect)} vs ${JSON.stringify(expected.rect)}`);
        proof.bounds.push({ name: expected.label, actual: rect, expected: expected.rect });
      }
      require(proof.bounds.length >= 3, "geometry oracle did not sample enough board items");
      proof.native_canvas_size = reference.canvas_size;
      proof.scene_size = reference.scene_size;
      require(reference.scene_size === expectedSize, "geometry oracle used the wrong scene size");
    }
    report[`${name}_geometry_proof`] = proof;
  };
  await geometryProof("initial");
  const width = root.style.width;
  const cases = geometryCase === "both" ? ["pan", "resize"] : [geometryCase];
  const change = async (name, index) => {
    const started = performance.now();
    if (name === "resize") root.style.width = index % 2 === 0 ? "1000px" : width;
    currentPan = name === "pan" && index % 2 === 0 ? size - 1 : undefined;
    const current = await frame(undefined, false, currentPan);
    current.request_elapsed_ms = performance.now() - started;
    return current;
  };
  for (const name of cases) {
    // Each case starts from the original camera and surface extent.
    root.style.width = width;
    currentPan = undefined;
    previous = await frame();
    if (settleMs > 0) await new Promise(resolve => setTimeout(resolve, settleMs));
    for (let index = 0; index < warmup; index++) previous = await change(name, index);
    const samples = [];
    report[`${name}_started_ms`] = Date.now();
    for (let index = 0; index < repeats; index++) {
      const current = await change(name, index);
      require(document.activeElement === button, `${name} moved accessibility focus`);
      require(current.sample.revision === initial.sample.revision, `${name} changed the scene revision`);
      require(current.sample.presentation > previous.sample.presentation, `${name} frame was not accepted`);
      require(current.sample.canvas_revision > previous.sample.canvas_revision, `${name} retained the previous canvas revision`);
      require(current.sample.rows_built === 0, `${name} rebuilt card row views`);
      require(current.sample.board_paint_ms > 0, `${name} did not repaint the changed viewport`);
      if (requireGeometryPublication) {
        require(current.sample.semantic_cards === 0, `${name} rebuilt ${current.sample.semantic_cards} board semantics`);
        require(current.publication_work.board_items_changed === 0, `${name} rewrote accessible card rectangles`);
        require(current.publication_work.proxy_replacements === 0, `${name} replaced accessibility proxies`);
        require(current.publication_work.board_mutations <= 8,
          `${name} made ${current.publication_work.board_mutations} board mutations`);
      }
      const measured = metrics(current);
      measured.request_elapsed_ms = current.request_elapsed_ms;
      samples.push(measured);
      if (index === 0) report[`${name}_proof`] = current;
      previous = current;
      if (samples.length % 25 === 0) await progress(name, samples);
    }
    report[`${name}_finished_ms`] = Date.now();
    report[name] = quantiles(samples);
    report[`${name}_samples`] = samples;
    await geometryProof(name);
    // Probe both extremes, including a viewport that is not the final timed one.
    await change(name, 0);
    await geometryProof(`${name}_alternate`);
    await change(name, 1);
  }
  root.style.width = width;
  currentPan = undefined;
  await frame();
  await geometryProof("restored");
  const transform = canvas.style.transform;
  canvas.style.transform = "translate(17px, 23px)";
  await frame();
  await geometryProof("moved_canvas");
  canvas.style.transform = transform;
  await frame();
  report.published_board_items = assertItems().length;
  if (params.get("ax_probe") === "required") {
    globalThis.__cardsGeometryAxReady = { size, names: originalNames, focus: "Graph commands" };
    globalThis.__cardsGeometryAxDone = false;
    await fetch("/scenario-progress", { method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ size, phase: "ax-ready" }) });
    for (let index = 0; index < 1200 && !globalThis.__cardsGeometryAxDone; index++) await tick();
    require(globalThis.__cardsGeometryAxDone, "browser accessibility-tree probe did not finish");
    report.browser_accessibility_tree_checked = true;
  }
  const edited = await frame(5);
  const editedName = label(5, true);
  require(mirrorItem(editedName, "Remote board") === initialItems[5], "label after geometry did not patch the existing board item");
  require(mirrorContains(editedName), "label after geometry is missing from Cards");
  require(document.activeElement === button, "label after geometry moved accessibility focus");
  const afterLabelPan = await frame(undefined, false, size - 1);
  currentPan = size - 1;
  if (requireGeometryPublication) {
    require(edited.sample.semantic_cards === 0, "label after geometry rebuilt board semantics");
    require(afterLabelPan.sample.semantic_cards === 0, "pan after label rebuilt board semantics");
    require(afterLabelPan.publication_work.board_items_changed === 0, "pan after label rewrote card rectangles");
  }
  require(mirrorItem(editedName, "Remote board") === initialItems[5], "pan after label lost the changed board name");
  require(document.activeElement === button, "pan after label moved accessibility focus");
  originalNames[5] = editedName;
  await geometryProof("pan_after_label");
  const otherSize = size === 1000 ? 5000 : 1000;
  const oldPlane = board().querySelector('[data-cambium-producer-plane]');
  report.remount_fallback = { other_size: otherSize, restored_size: size };
  graphshell.cards_receipt_mount(otherSize, compact, mode);
  currentPan = undefined;
  const other = await frame();
  report.remount_fallback.other_semantic_cards = other.sample.semantic_cards;
  report.remount_fallback.other_full_passes = other.sample.semantic_cards / otherSize;
  require(other.sample.semantic_cards >= otherSize, "scene remount did not publish complete new semantics");
  require(document.activeElement === button, "scene remount moved accessibility focus");
  if (requireGeometryPublication) require(oldPlane && !oldPlane.isConnected, "scene remount retained its old geometry plane");
  await geometryProof("remounted_other", { expectedSize: otherSize, retainOriginal: false });
  const otherPlane = board().querySelector('[data-cambium-producer-plane]');
  graphshell.cards_receipt_mount(size, compact, mode);
  currentPan = undefined;
  const restored = await frame();
  report.remount_fallback.restored_semantic_cards = restored.sample.semantic_cards;
  report.remount_fallback.restored_full_passes = restored.sample.semantic_cards / size;
  require(restored.sample.semantic_cards >= size, "scene restoration did not publish complete semantics");
  require(document.activeElement === button, "scene restoration moved accessibility focus");
  if (requireGeometryPublication) require(otherPlane && !otherPlane.isConnected, "scene restoration retained the previous geometry plane");
  currentPan = size - 1;
  const restoredPan = await frame(undefined, false, currentPan);
  if (requireGeometryPublication) {
    require(restoredPan.sample.semantic_cards === 0, "pan after remount rebuilt board semantics");
    require(restoredPan.publication_work.board_items_changed === 0, "pan after remount rewrote card rectangles");
    require(restoredPan.publication_work.proxy_replacements === 0, "pan after remount replaced proxies");
  }
  require(document.activeElement === button, "pan after scene restoration moved accessibility focus");
  await geometryProof("restored_after_remount", { retainOriginal: false });
  Object.assign(report.remount_fallback, { pan_semantic_cards: restoredPan.sample.semantic_cards,
    pan_board_items_changed: restoredPan.publication_work.board_items_changed,
    old_plane_detached: !oldPlane?.isConnected, focus_retained: document.activeElement === button });
  button.click();
  await tick();
  await tick();
  await frame();
  require(mirrorRoot.textContent.includes("Search graph commands") ||
    [...mirrorRoot.querySelectorAll('[aria-label]')].some(element => element.getAttribute("aria-label") === "Search graph commands"),
    "accessibility action after geometry did not open Graph commands");
  report.focus_and_action_after_geometry = true;
}

let result = "ok";
let log = [];
try {
  await run();
} catch (error) {
  result = "fail";
  log = [error?.stack ?? String(error)];
  report.diagnostics = { root: { ...root.dataset }, canvas: { ...root.querySelector("canvas")?.dataset } };
  report.last_attempt = lastAttempt;
}
document.title = `Remote Cards ${size}: ${result}`;
await fetch("/scenario-receipt", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ scenario: { result: { result, log } }, report }),
});
