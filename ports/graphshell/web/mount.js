// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// What any page needs to mount Graphshell: the host dataset fetch and the
// custom elements. No side effects; `viewer.js` (the plain viewer's entry)
// and `loader.js` (the receipt pages' harness) both import it.

// `?dataset=<url>`, else the root's `data-dataset-src`, names the page's host
// dataset: a `scenomise.host-dataset/v2` history or a v1 dataset. Its text
// becomes the root's `data-dataset`; a fetch that fails becomes
// `data-dataset-error`, which the page shows as a refusal rather than falling
// back to anything. A page may also set `data-dataset` itself. The page, not
// this script, parses and checks the envelope.
export async function loadHostDataset(root) {
  if (!root || root.hasAttribute("data-dataset") || root.hasAttribute("data-dataset-error")) return;
  const url =
    new URLSearchParams(location.search).get("dataset") ?? root.getAttribute("data-dataset-src");
  if (!url) return;
  try {
    const response = await fetch(url, { cache: "no-store" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    root.setAttribute("data-dataset", await response.text());
    root.removeAttribute("data-dataset-error");
  } catch (error) {
    root.removeAttribute("data-dataset");
    root.setAttribute("data-dataset-error", `could not fetch ${url}: ${error.message ?? error}`);
  }
}

// The read-only viewer ships one artifact, `pkg/webgl2/` (the WebGL2 presenter,
// built without Classic Vello), for every browser, with or without WebGPU
// (Mark, 2026-11-08, after the WebGL2 receipts: see the host plan, T7). Auto,
// `webgl2` and the element's own policy all load it. `pkg/webgpu/` exists only as
// an opt-in comparison build (`build-viewer.py --with-webgpu`), loaded only by an
// explicit `?render_backend=webgpu`; without it that request fails to load.
const artifacts = new Map();
export function loadArtifact(name) {
  if (!artifacts.has(name)) {
    artifacts.set(
      name,
      import(`./pkg/${name}/graphshell_web.js`).then(async (module) => {
        await module.default();
        return module;
      }),
    );
  }
  return artifacts.get(name);
}
export function chooseArtifact(choice) {
  return choice === "webgpu" ? "webgpu" : "webgl2";
}
export async function viewerArtifact(choice) {
  const artifact = chooseArtifact(choice);
  return { artifact, module: await loadArtifact(artifact) };
}

// A build without the force engine runs its physics only in the Worker. When the
// page says it has none (`data-physics-backend="unavailable"`, with the reason in
// `data-physics-unavailable`: the Worker did not start, or died), the element
// says so itself, visibly and to assistive technology, instead of leaving a graph
// that silently does not move. The graph stays where it was first drawn.
function watchPhysics(element) {
  element._physicsWatch?.disconnect();
  const show = () => {
    let note = element.querySelector(":scope > [data-physics-notice]");
    if (element.getAttribute("data-physics-backend") !== "unavailable") {
      note?.remove();
      return;
    }
    if (!note) {
      note = document.createElement("div");
      note.setAttribute("data-physics-notice", "");
      note.setAttribute("role", "status");
      note.setAttribute("aria-live", "polite");
      note.style.cssText =
        "position:absolute;left:8px;right:8px;top:8px;z-index:2;padding:8px 12px;" +
        "font:14px/1.4 sans-serif;background:#2a1d08;color:#ffe3b3;border:1px solid #b8873a;" +
        "border-radius:4px;pointer-events:none";
      if (getComputedStyle(element).position === "static") element.style.position = "relative";
      element.append(note);
    }
    const reason = element.getAttribute("data-physics-unavailable");
    note.textContent =
      "Physics unavailable: this page could not start its layout worker, so the graph stays " +
      "where it was first drawn. Reload to try again." + (reason ? ` (${reason})` : "");
  };
  element._physicsWatch = new MutationObserver(show);
  element._physicsWatch.observe(element, {
    attributes: true,
    attributeFilter: ["data-physics-backend", "data-physics-unavailable"],
  });
  show();
}

// Two elements, one retained tree. `mount` remains the full bundle's public
// compatibility entry and delegates to `mount_tree`; the viewer-only bundle
// exposes `mount_tree`. Each fetches its dataset, if it names one, before it
// mounts. Elements already in the document upgrade on define. `source` is a
// loaded module, or `viewerArtifact`, which loads the viewer's
// artifact per element policy (and so defines only `graphshell-tree`).
export function defineElements(source) {
  const loads = typeof source === "function";
  const define = (name, entry) => {
    if ((!loads && !source[entry]) || customElements.get(name)) return;
    customElements.define(
      name,
      class extends HTMLElement {
        async connectedCallback() {
          if (this.dataset.mounted) return;
          this.dataset.mounted = "true";
          // Where the physics Worker's script is, for a page that runs its
          // simulation there (`?physics=worker`).
          this.setAttribute("data-physics-worker", new URL("./pkg/physics/worker.js", import.meta.url).href);
          watchPhysics(this);
          const epoch = this._mountEpoch = (this._mountEpoch ?? 0) + 1;
          try {
            const choice = new URLSearchParams(location.search).get("render_backend")
              ?? this.getAttribute("data-render-backend") ?? "auto";
            if (!["auto", "webgpu", "webgl2"].includes(choice)) {
              throw new Error(`invalid render_backend: ${choice}`);
            }
            this.setAttribute("data-render-backend", choice);
            await loadHostDataset(this);
            if (!this.isConnected || this._mountEpoch !== epoch) return;
            const loaded = loads ? await source(choice) : { module: source };
            if (!this.isConnected || this._mountEpoch !== epoch) return;
            this._attach(loaded, choice, epoch);
          } catch (error) {
            if (this._mountEpoch === epoch) {
              delete this.dataset.mounted;
              this.setAttribute("data-mount-error", String(error.message ?? error));
              throw error;
            }
          }
        }
        _attach({ module, artifact }, choice, epoch) {
          this.graphshellModule = module;
          if (artifact) this.setAttribute("data-viewer-artifact", artifact);
          module[entry](this);
        }
        disconnectedCallback() {
          this._mountEpoch = (this._mountEpoch ?? 0) + 1;
          this._physicsWatch?.disconnect();
          this.graphshellModule?.dispose_tree?.(this);
          delete this.dataset.mounted;
          this.removeAttribute("data-ready");
        }
      },
    );
  };
  if (!loads) define("graphshell-view", "mount");
  define("graphshell-tree", "mount_tree");
}
