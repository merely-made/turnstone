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

// Two elements, one bundle. `<graphshell-tree>` is the one-tree page and the
// plain viewer; `<graphshell-view>` is the H5 main page, defined only when the
// bundle has it (not in a `--no-default-features` viewer). Each fetches its
// dataset, if it names one, before it mounts. Elements already in the
// document upgrade on define.
export function defineElements(module) {
  const define = (name, mount) => {
    if (!mount || customElements.get(name)) return;
    customElements.define(
      name,
      class extends HTMLElement {
        async connectedCallback() {
          if (this.dataset.mounted) return;
          this.dataset.mounted = "true";
          await loadHostDataset(this);
          mount(this);
        }
      },
    );
  };
  define("graphshell-view", module.mount);
  define("graphshell-tree", module.mount_tree);
}
