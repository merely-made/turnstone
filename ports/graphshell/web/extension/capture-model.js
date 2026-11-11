// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

(() => {
  const DEFAULT_QUEUE_LIMIT = 2048;

  function defaultPolicy() {
    return {
      enabled: false,
      accepted_schemes: ["http", "https"],
      excluded_origins: [],
      strip_query: false,
      strip_fragment: true,
      dedupe_window_ms: 1000,
      segment_size: 64,
      retention_traces: 2048,
      encounter_limit: 100000,
    };
  }

  function boundedInteger(value, fallback, minimum, maximum) {
    return Number.isInteger(value)
      ? Math.min(maximum, Math.max(minimum, value))
      : fallback;
  }

  function normalizePolicy(value) {
    const fallback = defaultPolicy();
    const acceptedSchemes = Array.isArray(value?.accepted_schemes)
      ? value.accepted_schemes
          .map((scheme) => String(scheme).trim().toLowerCase())
          .filter(Boolean)
      : fallback.accepted_schemes;
    const excludedOrigins = Array.isArray(value?.excluded_origins)
      ? value.excluded_origins
          .map((origin) => {
            try {
              return new URL(String(origin).trim()).origin;
            } catch {
              return null;
            }
          })
          .filter(Boolean)
      : fallback.excluded_origins;
    return {
      enabled: value?.enabled === true,
      accepted_schemes: [...new Set(acceptedSchemes)],
      excluded_origins: [...new Set(excludedOrigins)],
      strip_query: value?.strip_query === true,
      strip_fragment: value?.strip_fragment !== false,
      dedupe_window_ms: boundedInteger(
        value?.dedupe_window_ms,
        fallback.dedupe_window_ms,
        0,
        60_000,
      ),
      segment_size: boundedInteger(
        value?.segment_size,
        fallback.segment_size,
        1,
        1024,
      ),
      retention_traces: boundedInteger(
        value?.retention_traces,
        fallback.retention_traces,
        1,
        100_000,
      ),
      encounter_limit: boundedInteger(value?.encounter_limit, fallback.encounter_limit, 1, 1_000_000),
    };
  }

  function sanitizeAddress(address, policy) {
    try {
      const url = new URL(String(address));
      const scheme = url.protocol.slice(0, -1).toLowerCase();
      if (!policy.accepted_schemes.includes(scheme)) {
        return null;
      }
      if (policy.excluded_origins.includes(url.origin)) {
        return null;
      }
      if (policy.strip_query) {
        url.search = "";
      }
      if (policy.strip_fragment) {
        url.hash = "";
      }
      return url.href;
    } catch {
      return null;
    }
  }

  function optionalText(value) {
    const text = typeof value === "string" ? value.trim() : "";
    return text || null;
  }

  function normalizeHistoryFilter(value) {
    const start = Number.isFinite(value?.start_ms)
      ? Math.max(0, Math.trunc(value.start_ms))
      : null;
    const end = Number.isFinite(value?.end_ms)
      ? Math.max(0, Math.trunc(value.end_ms))
      : null;
    return {
      start_ms: start,
      end_ms: end !== null && start !== null && end <= start ? null : end,
      persona: optionalText(value?.persona),
      device: optionalText(value?.device),
    };
  }

  function historyFilterFromControls(days, persona, device, nowMs = Date.now()) {
    const boundedDays = boundedInteger(days, 0, 0, 3650);
    const end = Math.max(0, Math.trunc(nowMs));
    return normalizeHistoryFilter({
      start_ms: boundedDays > 0 ? end - boundedDays * 86_400_000 : null,
      end_ms: null,
      persona,
      device,
    });
  }

  function forgetRequest(address, removeObject, configuredPolicy) {
    const policy = normalizePolicy(configuredPolicy);
    try {
      const url = new URL(String(address).trim());
      const scheme = url.protocol.slice(0, -1).toLowerCase();
      if (!policy.accepted_schemes.includes(scheme)) {
        return null;
      }
      if (policy.strip_query) {
        url.search = "";
      }
      if (policy.strip_fragment) {
        url.hash = "";
      }
      return {
        url: url.href,
        remove_object: removeObject === true,
      };
    } catch {
      return null;
    }
  }

  function sanitizeVisit(value, configuredPolicy) {
    const policy = normalizePolicy(configuredPolicy);
    if (!policy.enabled || value?.private === true) {
      return null;
    }
    const url = sanitizeAddress(value?.url, policy);
    if (!url) {
      return null;
    }
    const referrer = optionalText(value?.referrer_url);
    const referrerUrl = referrer ? sanitizeAddress(referrer, policy) : null;
    const scope = value?.source_scope;
    const sourceScope = scope && [scope.product, scope.profile, scope.generation]
      .every((part) => typeof part === "string" && part.trim() && part.length <= 256)
      ? { product: scope.product, profile: scope.profile, generation: scope.generation } : null;
    if (scope && !sourceScope) return null;
    const sourceParent = value?.parent;
    let parent = { kind: "unknown" };
    if (["unknown", "root", "withheld"].includes(sourceParent?.kind)) {
      parent = { kind: sourceParent.kind };
    } else if (["visit", "outside_range"].includes(sourceParent?.kind) && optionalText(sourceParent.visit_id)) {
      parent = { kind: sourceParent.kind, visit_id: sourceParent.visit_id };
    }
    if (referrer && !referrerUrl) parent = { kind: "withheld" };
    return {
      source: optionalText(value?.source)?.toLowerCase() ?? "browser",
      visit_id: optionalText(value?.visit_id),
      source_scope: sourceScope,
      parent,
      original_unix_micros: Number.isSafeInteger(value?.original_unix_micros) ? value.original_unix_micros : null,
      source_is_local: typeof value?.source_is_local === "boolean" ? value.source_is_local : null,
      url,
      title: optionalText(value?.title),
      favicon_url: optionalText(value?.favicon_url),
      referrer_url: referrerUrl,
      transition: optionalText(value?.transition) ?? "unknown",
      at_ms: Number.isFinite(value?.at_ms) ? Math.max(0, Math.trunc(value.at_ms)) : 0,
      private: false,
    };
  }

  function queueKey(visit) {
    const source = visit.source_scope ? JSON.stringify(visit.source_scope) : visit.source;
    return visit.visit_id
      ? `${source}\u0000${visit.visit_id}`
      : `${visit.source}\u0000${visit.url}\u0000${visit.at_ms}\u0000${visit.transition}`;
  }

  // The ledger's `same_source_facts` (eidetic import/encounters.rs): a visit
  // is a replay of its identity when these agree. Titles may differ. A visit
  // or outside-range parent with the same visit id is one parent.
  function parentKey(parent) {
    return ["visit", "outside_range"].includes(parent?.kind)
      ? `visit:${parent.visit_id}` : parent?.kind ?? "unknown";
  }

  function sameSourceFacts(left, right) {
    return left.url === right.url && left.at_ms === right.at_ms
      && left.transition === right.transition
      && parentKey(left.parent) === parentKey(right.parent)
      && (left.original_unix_micros ?? null) === (right.original_unix_micros ?? null)
      && (left.source_is_local ?? null) === (right.source_is_local ?? null);
  }

  // The first visit for an identity wins; a later visit with other source
  // facts is refused alone and returned, never thrown over the whole batch.
  function mergeQueue(existing, incoming, limit = DEFAULT_QUEUE_LIMIT) {
    const merged = new Map();
    const refused = [];
    for (const visit of [...existing, ...incoming]) {
      const key = queueKey(visit);
      const prior = merged.get(key);
      if (!prior) {
        merged.set(key, visit);
      } else if (!sameSourceFacts(prior, visit)) {
        refused.push(visit);
      }
    }
    const queue = [...merged.values()]
      .sort((left, right) => left.at_ms - right.at_ms)
      .slice(-boundedInteger(limit, DEFAULT_QUEUE_LIMIT, 1, 100_000));
    return { queue, refused };
  }


  // ---- Kept page text (application experience plan, "Keep page text") ----
  // A control separate from live remembering. Page text is extracted in the
  // page the person loaded, bounded here, queued beside the visit queue, and
  // recorded by the writer view into Eidetic's PageTextStore.
  const KEEP_DEFAULT_BYTES = 128 * 1024;
  const KEEP_HARD_BYTES = 256 * 1024;
  const KEEP_QUEUE_LIMIT = 48;
  const KEEP_QUEUE_BYTES = 6 * 1024 * 1024;
  const KEEP_PULL_PAGES = 8;
  const KEEP_PULL_BYTES = 1024 * 1024;

  function defaultKeepPolicy() {
    return { enabled: false, max_bytes: KEEP_DEFAULT_BYTES };
  }

  function normalizeKeepPolicy(value) {
    return {
      enabled: value?.enabled === true,
      max_bytes: boundedInteger(value?.max_bytes, KEEP_DEFAULT_BYTES, 1024, KEEP_HARD_BYTES),
    };
  }

  function byteLength(text) {
    return new TextEncoder().encode(text).length;
  }

  // The longest prefix of `text` that fits `maxBytes` of UTF-8, never
  // splitting a surrogate pair. `truncated` says whether anything was cut.
  function boundText(text, maxBytes) {
    if (byteLength(text) <= maxBytes) return { text, truncated: false };
    let low = 0;
    let high = Math.min(text.length, maxBytes);
    while (low < high) {
      const mid = Math.ceil((low + high) / 2);
      if (byteLength(text.slice(0, mid)) <= maxBytes) low = mid; else high = mid - 1;
    }
    const lastUnit = text.charCodeAt(low - 1);
    if (lastUnit >= 0xd800 && lastUnit <= 0xdbff) low -= 1;
    return { text: text.slice(0, low).trimEnd(), truncated: true };
  }

  // The address a kept page is filed under, or null when capture policy keeps
  // it out: an unaccepted scheme or an excluded origin. Live remembering being
  // off does not lift these; they are the person's exclusions.
  function keepAddress(address, configuredPolicy) {
    return sanitizeAddress(address, { ...normalizePolicy(configuredPolicy), enabled: true });
  }

  function sanitizeKeptPage(value, keepPolicy, configuredPolicy) {
    const keep = normalizeKeepPolicy(keepPolicy);
    if (!keep.enabled || value?.private === true || typeof value?.text !== "string") return null;
    const url = keepAddress(value?.url, configuredPolicy);
    if (!url) return null;
    const normalized = value.text.replace(/\s+/g, " ").trim();
    if (!normalized) return null;
    const bounded = boundText(normalized, keep.max_bytes);
    return {
      url,
      text: bounded.text,
      truncated: bounded.truncated,
      at_ms: Number.isFinite(value?.at_ms) ? Math.max(0, Math.trunc(value.at_ms)) : 0,
      private: false,
    };
  }

  function keepKey(page) {
    return `${page.url}\u0000${page.at_ms}`;
  }

  // One pending text per address: loading a page again replaces its earlier
  // pending text. Past the count or byte bound the oldest pending pages are
  // dropped and counted, so the person is told coverage is partial.
  function mergeKeepQueue(existing, incoming, limit = KEEP_QUEUE_LIMIT, byteLimit = KEEP_QUEUE_BYTES) {
    const merged = new Map();
    for (const page of [...existing, ...incoming]) merged.set(page.url, page);
    let queue = [...merged.values()].sort((left, right) => left.at_ms - right.at_ms);
    let omitted = Math.max(0, queue.length - limit);
    queue = queue.slice(omitted);
    let bytes = queue.reduce((sum, page) => sum + byteLength(page.text), 0);
    while (queue.length > 1 && bytes > byteLimit) {
      bytes -= byteLength(queue[0].text);
      queue = queue.slice(1);
      omitted += 1;
    }
    return { queue, omitted };
  }

  // The next delivery: oldest first, a few pages and about a megabyte.
  function takeKeepDelivery(queue) {
    const pages = [];
    let bytes = 0;
    for (const page of queue) {
      const size = byteLength(page.text);
      if (pages.length > 0 && (pages.length >= KEEP_PULL_PAGES || bytes + size > KEEP_PULL_BYTES)) break;
      pages.push(page);
      bytes += size;
    }
    return pages;
  }

  // Runs in the loaded page (`scripting.executeScript`), so it must stand
  // alone: no reference to anything outside its own body. Reads what the page
  // already shows: visible text of main/article (else the body), skipping
  // scripts, styles, navigation, forms, hidden and embedded content. Never
  // fetches. Returns null for a document that is not HTML.
  function extractPageText(maxChars) {
    const doc = document;
    if (!/^(text\/html|application\/xhtml\+xml)$/.test(doc.contentType)) return null;
    const skip = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "SVG", "IFRAME", "CANVAS",
      "OBJECT", "EMBED", "TEXTAREA", "SELECT", "OPTION", "INPUT", "BUTTON", "NAV", "ASIDE",
      "FOOTER", "FORM", "DIALOG"]);
    const candidates = [doc.querySelector("main"), doc.querySelector("[role=main]"),
      doc.querySelector("article")];
    const root = candidates.find((node) => node && (node.textContent || "").trim().length > 200)
      ?? doc.body;
    if (!root) return null;
    const verdicts = new Map();
    const readable = (element) => {
      if (!element || element === root.parentNode) return true;
      if (verdicts.has(element)) return verdicts.get(element);
      let ok = !skip.has(element.tagName.toUpperCase()) && !element.hidden
        && element.getAttribute("aria-hidden") !== "true"
        && (typeof element.checkVisibility !== "function"
          || element.checkVisibility({ visibilityProperty: true }));
      if (ok && element !== root) ok = readable(element.parentElement);
      verdicts.set(element, ok);
      return ok;
    };
    const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const parts = [];
    let length = 0;
    while (length < maxChars && walker.nextNode()) {
      const text = walker.currentNode.nodeValue;
      if (!text || !text.trim() || !readable(walker.currentNode.parentElement)) continue;
      parts.push(text.trim());
      length += text.trim().length + 1;
    }
    return { url: location.href, title: doc.title || "", text: parts.join(" ").slice(0, maxChars) };
  }

  globalThis.GraphshellCaptureModel = Object.freeze({
    DEFAULT_QUEUE_LIMIT,
    defaultPolicy,
    normalizePolicy,
    normalizeHistoryFilter,
    historyFilterFromControls,
    forgetRequest,
    sanitizeVisit,
    queueKey,
    mergeQueue,
    KEEP_DEFAULT_BYTES,
    KEEP_HARD_BYTES,
    defaultKeepPolicy,
    normalizeKeepPolicy,
    boundText,
    keepAddress,
    sanitizeKeptPage,
    keepKey,
    mergeKeepQueue,
    takeKeepDelivery,
    extractPageText,
  });
})();
