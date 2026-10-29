// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

import assert from "node:assert/strict";
import fs from "node:fs";
import "../extension/capture-model.js";

const model = globalThis.GraphshellCaptureModel;
const policy = model.normalizePolicy({
  ...model.defaultPolicy(),
  enabled: true,
  strip_query: true,
  excluded_origins: ["https://private.example/path"],
});
const accepted = model.sanitizeVisit(
  {
    source: "Firefox",
    visit_id: "visit-1",
    url: "https://example.net/research?token=secret#part",
    title: " Research ",
    favicon_url: null,
    referrer_url: "https://example.net/from?private=1#part",
    transition: "LINK",
    at_ms: 42,
    private: false,
  },
  policy,
);
assert.equal(accepted.url, "https://example.net/research");
assert.equal(accepted.referrer_url, "https://example.net/from");
assert.equal(accepted.title, "Research");
assert.equal(accepted.transition, "LINK");
assert.equal(
  model.sanitizeVisit(
    {
      ...accepted,
      url: "https://private.example/a",
    },
    policy,
  ),
  null,
);
assert.equal(model.sanitizeVisit({ ...accepted, private: true }, policy), null);
assert.deepEqual(
  model.mergeQueue([accepted], [{ ...accepted, title: "Updated" }]),
  { queue: [accepted], refused: [] },
  "a replay with another title is a duplicate",
);

// A visit whose source facts conflict with its identity's first visit is
// refused alone; the rest of the batch still merges.
const visit = (id, change = {}) => ({ ...accepted, visit_id: id, at_ms: 100, ...change });
const first = visit("a");
const conflicting = visit("a", { url: "https://example.net/other" });
const fresh = [visit("b", { at_ms: 200 }), visit("c", { at_ms: 300 })];
const batch = model.mergeQueue([first], [fresh[0], conflicting, fresh[1]]);
assert.deepEqual(batch.refused, [conflicting], "only the conflicting visit is refused");
assert.deepEqual(batch.queue, [first, ...fresh], "the first visit stays and the rest of the batch merges");
const twice = model.mergeQueue([], [first, conflicting]);
assert.deepEqual(twice, { queue: [first], refused: [conflicting] }, "the earlier visit of a batch wins");

// The conflict rule is the encounter ledger's: the fixture holds visit pairs
// with the verdict `BrowserEncounterLedger::receive` gave them
// (tests/capture_conflict_fixture.rs writes and checks it).
const fixture = JSON.parse(fs.readFileSync(new URL("./fixtures/capture-conflicts.json", import.meta.url), "utf8"));
assert.ok(fixture.length >= 10, "the fixture covers each source fact");
for (const { name, first: prior, second, verdict } of fixture) {
  assert.deepEqual(model.sanitizeVisit(prior, policy), prior, `${name}: first is a queued visit`);
  assert.deepEqual(model.sanitizeVisit(second, policy), second, `${name}: second is a queued visit`);
  const merged = model.mergeQueue([prior], [second]);
  const reached = merged.refused.length ? "conflict" : merged.queue.length > 1 ? "new" : "duplicate";
  assert.equal(reached, verdict, `${name}: mergeQueue and the ledger agree`);
  assert.deepEqual(merged.queue[0], prior, `${name}: the first visit is kept`);
  if (verdict === "conflict") assert.deepEqual(merged.refused, [second], `${name}: the second is refused`);
}
assert.deepEqual(
  model.historyFilterFromControls(7, " persona:a ", " device:b ", 700_000_000),
  {
    start_ms: 95_200_000,
    end_ms: null,
    persona: "persona:a",
    device: "device:b",
  },
);
assert.deepEqual(
  model.forgetRequest(
    "https://private.example/a?token=secret#part",
    true,
    policy,
  ),
  {
    url: "https://private.example/a",
    remove_object: true,
  },
  "an excluded origin stays forgettable after capture",
);
console.log("Graphshell capture policy smoke passed");
