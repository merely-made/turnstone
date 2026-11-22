// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

// The query page (graphshell:mere-cli D21). The join, the session and the rows
// are Rust's (`query_remote`); this page only reads the invitation, fetches the
// allowed query from the owner's signaling (D18), and hands both over.
const $ = (id) => document.getElementById(id);

// `#signal=…&invite=…` fills the form, so an owner can send one link.
const hash = new URLSearchParams(location.hash.slice(1));
if (hash.get("signal")) $("signal").value = hash.get("signal");
if (hash.get("invite")) $("fragment").value = hash.get("invite");

let allowed = null;

$("invite-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const signal = $("signal").value.trim().replace(/\/+$/, "");
  $("status").textContent = "fetching the allowed query";
  try {
    const response = await fetch(`${signal}/query`);
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    allowed = await response.text();
    $("query-text").textContent = allowed;
    $("query-box").hidden = false;
    $("status").textContent = "ready to run";
  } catch (error) {
    $("status").textContent = `Failed · GET /query: ${error.message}`;
  }
});

$("run").addEventListener("click", async () => {
  if (allowed === null) return;
  $("run").disabled = true;
  const module = await import("./pkg/graphshell_web.js");
  await module.default();
  const fragment = $("fragment").value.trim();
  try {
    module.query_remote($("signal").value.trim(), fragment || undefined, allowed);
  } catch (error) {
    $("status").textContent = `Failed · ${error}`;
    $("run").disabled = false;
  }
});
