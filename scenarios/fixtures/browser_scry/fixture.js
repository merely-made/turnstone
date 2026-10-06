// Copyright 2026 Mark Alan Boykin. SPDX-License-Identifier: MPL-2.0
"use strict";
(() => {
  const page = document.documentElement.dataset.page;
  const entry = document.getElementById("entry");
  const composition = document.getElementById("composition");
  const button = document.getElementById("activate");
  const prefix = `turnstone.scry.${page}.`;
  const events = [];
  let clicks = 0;
  let wheels = 0;
  let sequence = 0;
  let stored = "";
  let storageError = "";
  let loads = 0;
  try {
    stored = localStorage.getItem(prefix + "marker") || "";
    entry.value = localStorage.getItem(prefix + "text") || "";
    loads = Number(localStorage.getItem(prefix + "loads") || 0) + 1;
    localStorage.setItem(prefix + "loads", String(loads));
  } catch (error) { storageError = String(error); }

  function snapshot() {
    return {
      fixture: "turnstone-browser-scry-v1", page, sequence, clicks, wheels,
      text: entry.value, compositionText: composition.value,
      focus: document.activeElement && document.activeElement.id || "body",
      viewport: [innerWidth, innerHeight], dpr: devicePixelRatio,
      scroll: [scrollX, scrollY], stored, loads, cookie: document.cookie,
      storageError, events: events.slice()
    };
  }
  function publish(reason, event) {
    sequence += 1;
    if (event) {
      events.push({ sequence, type: event.type, target: event.target.id || "body",
        key: event.key || "", data: event.data || "", trusted: event.isTrusted });
      if (events.length > 12) events.shift();
    }
    const state = snapshot();
    const title = `Scry ${page} | text=${encodeURIComponent(entry.value)} | clicks=${clicks} | stored=${stored}`
      + ` | cookie=${state.cookie} | viewport=${innerWidth}x${innerHeight}@${devicePixelRatio}`
      + ` | scroll=${Math.round(scrollX)},${Math.round(scrollY)} | scrolled=${scrollX !== 0 || scrollY !== 0 ? "yes" : "no"} | wheels=${wheels}`;
    document.title = title;
    document.getElementById("summary").textContent = title;
    document.getElementById("state").textContent = JSON.stringify({ ...state, events: undefined }, null, 2);
    document.getElementById("events").textContent = JSON.stringify(events, null, 2);
    // This diagnostic is optional. The visible state and title work through
    // ordinary browser APIs even when the host does not project WebMessage.
    if (window.chrome && window.chrome.webview) {
      window.chrome.webview.postMessage(JSON.stringify({ reason, ...state }));
    }
  }
  button.addEventListener("click", event => {
    clicks += 1;
    try {
      stored = `${page}-retained`;
      localStorage.setItem(prefix + "marker", stored);
      localStorage.setItem(prefix + "text", entry.value);
      document.cookie = `scry_${page}=${page}-retained; Path=/; SameSite=Lax; Max-Age=86400`;
    } catch (error) { storageError = String(error); }
    publish("activate", event);
  });
  for (const type of ["input", "keydown", "keyup", "focusin", "focusout",
    "compositionstart", "compositionupdate", "compositionend", "pointerdown", "pointerup"]) {
    document.addEventListener(type, event => publish(type, event));
  }
  window.addEventListener("resize", event => publish("resize", event));
  window.addEventListener("scroll", event => publish("scroll", event));
  window.addEventListener("wheel", event => {
    wheels += 1;
    publish("wheel", event);
  }, { passive: true });
  window.browserScryFixture = Object.freeze({ snapshot });
  publish("ready");
})();
