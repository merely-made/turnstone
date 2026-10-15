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

// The read-only viewer ships one artifact per presentation route (`pkg/webgpu`,
// `pkg/webgl2`), so a page loads only the renderer it presents with. An
// explicit policy loads that route. Auto loads WebGPU when an adapter and a
// device are actually available, else WebGL2; if the WebGPU artifact still
// refuses at boot, the element remounts on WebGL2, as Auto does in one bundle.
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
export async function chooseArtifact(choice) {
  if (choice === "webgpu" || choice === "webgl2") return choice;
  try {
    const adapter = await navigator.gpu?.requestAdapter();
    const device = await adapter?.requestDevice();
    if (device) {
      device.destroy();
      return "webgpu";
    }
  } catch {}
  return "webgl2";
}
export async function viewerArtifact(choice) {
  const artifact = await chooseArtifact(choice);
  return { artifact, module: await loadArtifact(artifact) };
}

// A WebGPU-only artifact reports Auto's second attempt as not compiled in.
const WEBGPU_ONLY_REFUSAL = "WebGL2: not compiled into this host";

// Two elements, one retained tree. `mount` remains the full bundle's public
// compatibility entry and delegates to `mount_tree`; the viewer-only bundle
// exposes `mount_tree`. Each fetches its dataset, if it names one, before it
// mounts. Elements already in the document upgrade on define. `source` is a
// loaded module, or `viewerArtifact`, which chooses and loads a route
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
          if (artifact === "webgpu" && choice === "auto") {
            this._fallback?.disconnect();
            this._fallback = new MutationObserver(async () => {
              const error = this.getAttribute("data-mount-error") ?? "";
              if (!error.includes(WEBGPU_ONLY_REFUSAL) || this._mountEpoch !== epoch) return;
              this._fallback.disconnect();
              module.dispose_tree?.(this);
              this.removeAttribute("data-mount-error");
              const webgl2 = await source("webgl2");
              if (this.isConnected && this._mountEpoch === epoch) this._attach(webgl2, choice, epoch);
            });
            this._fallback.observe(this, { attributes: true, attributeFilter: ["data-mount-error"] });
          }
          module[entry](this);
        }
        disconnectedCallback() {
          this._mountEpoch = (this._mountEpoch ?? 0) + 1;
          this._fallback?.disconnect();
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
