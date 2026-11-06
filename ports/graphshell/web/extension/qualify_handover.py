#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Exercise the installed extension's in-flight edit handover in headed browsers.

Call scenario(browser, url, receipt, out, *, helpers) from a browser harness
with a fresh installed profile. For a standalone run, pass --out.
"""

import argparse
import base64
from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path
import platform
import plistlib
import tempfile
import traceback
import uuid


LOCK = "graphshell.local-profile.writer.v1"
BUSY = "That view has a change being saved or awaiting retry. Finish it there first."

# Forward real product transactions unchanged and observe their lifecycle.
# Install in the writer only, after mount and before submitting the edit.
WATCH_WRITES = """(() => {
  const writes = globalThis.handoverWrites = [];
  const transaction = IDBDatabase.prototype.transaction;
  IDBDatabase.prototype.transaction = function (...args) {
    const tx = Reflect.apply(transaction, this, args);
    if (this.name === "graphshell-local-v1" && tx.mode === "readwrite"
        && tx.objectStoreNames.contains("muniment")) {
      const write = {stores: [...tx.objectStoreNames], outcome: "pending", requests: []};
      writes.push(write);
      tx.addEventListener("complete", () => { write.outcome = "complete"; });
      tx.addEventListener("abort", () => { write.outcome = "abort"; });
      for (const type of ["success", "error"]) {
        // Request errors bubble. Successes are observable on the capture path.
        tx.addEventListener(type, event => write.requests.push(event.type), true);
      }
    }
    return tx;
  };
  return true;
})()"""

# An overlapping native readwrite transaction queues the writer's real save.
# Keep it active with read requests. Releasing stops the pump and lets the
# browser commit this read-only workload and run the queued save normally.
BARRIER = """(() => {
  const barrier = globalThis.handoverBarrier = {phase: "opening", reads: 0, release: false};
  const open = indexedDB.open("graphshell-local-v1");
  open.onupgradeneeded = () => open.transaction.abort();
  open.onerror = () => { barrier.phase = "open-error"; };
  open.onsuccess = () => {
    const db = open.result;
    const tx = db.transaction("muniment", "readwrite");
    const store = tx.objectStore("muniment");
    tx.onabort = () => { barrier.phase = "aborted"; db.close(); };
    tx.oncomplete = () => { barrier.phase = "complete"; db.close(); };
    const deadline = performance.now() + 12000;
    function pump() {
      store.get("__qualification_handover_barrier__").onsuccess = () => {
        barrier.reads++;
        if (barrier.release) return;
        if (performance.now() >= deadline) {
          barrier.expired = true;
          return;
        }
        barrier.phase = "holding";
        pump();
      };
    }
    pump();
  };
  return true;
})()"""

WATCH_FOLLOWER = """(() => {
  const probe = globalThis.handoverFollower = {messages: []};
  const channel = new BroadcastChannel("graphshell.local-profile.views.v1");
  channel.onmessage = event => probe.messages.push(event.data);
  probe.sample = () => navigator.locks.query().then(locks => { probe.locks = locks; });
  return true;
})()"""


def scenario(browser, url, receipt, out, *, helpers):
    """Run the live two-tab gate. Caller owns launch, receipt and restart."""
    h = helpers
    receipt.update(
        scenario="follower-edit-in-flight-handover",
        barrier="Native IndexedDB transaction on graphshell-local-v1/muniment in follower. "
                "A read pump holds the overlapping readwrite scope until release. "
                "Writer transaction wrapper forwards native calls and observes events.",
    )
    writer = browser.open(url)
    h.observe(browser, writer, "writer_initial", lambda s: h.ready(s, "writer", "visible"), receipt)
    browser.evaluate(writer, h.PROBE)
    follower = browser.open(url)
    h.observe(browser, follower, "follower_initial", lambda s: h.ready(s, "follower", "visible"), receipt)
    browser.evaluate(follower, h.PROBE)
    h.observe(browser, writer, "writer_before", lambda s: h.ready(s, "writer", "hidden"), receipt)
    browser.evaluate(follower, WATCH_FOLLOWER)

    def click(label):
        h.wait(label, lambda: browser.evaluate(follower, h.button(label)), timeout=12)

    def locks(label):
        browser.evaluate(follower, "(delete handoverFollower.locks, handoverFollower.sample(), true)")
        snapshot = h.wait(label, lambda: browser.evaluate(follower, "handoverFollower.locks"), timeout=5)
        receipt[label] = snapshot
        held = [lock for lock in snapshot["held"] if lock["name"] == LOCK]
        pending = [lock for lock in snapshot["pending"] if lock["name"] == LOCK]
        h.require(len(held) == 1 and held[0]["mode"] == "exclusive" and not pending,
                  f"{label}: expected one exclusive writer and no queued handover: {snapshot}")
        return held[0]["clientId"]

    def screenshot(name):
        (out / f"{name}.png").write_bytes(base64.b64decode(browser.screenshot(follower)))

    original_client = locks("locks_before")
    writer_document = browser.evaluate(writer, "performance.timeOrigin")
    follower_document = browser.evaluate(follower, "performance.timeOrigin")
    title = receipt["title"]
    h.require(h.labels(receipt["follower_initial"]["semantic"], title) == 0, "Object already present")
    click("Panes")
    click("Document tools")
    h.observe(browser, follower, "document_tools_open",
              lambda s: h.labels(s["semantic"], "Add an object"), receipt, timeout=12)
    click("Add an object")
    for label, value in [("Address", receipt["address"]), ("New object title", title)]:
        h.wait(f"Enter {label}", lambda: browser.evaluate(follower, h.field(label, value)), timeout=12)
        selector = json.dumps(f'[role="textbox"][aria-label="{label}"]')
        h.wait(f"Confirm {label}", lambda: browser.evaluate(
            follower, f"document.querySelector({selector})?.textContent === {json.dumps(value)}"), timeout=12)

    browser.evaluate(writer, WATCH_WRITES)
    browser.evaluate(follower, BARRIER)
    try:
        h.wait("Native barrier active", lambda: browser.evaluate(
            follower, 'handoverBarrier.phase === "holding"'), timeout=5)
        click("New node")

        def blocked():
            state = browser.evaluate(writer, h.STATE)
            writes = browser.evaluate(writer, "handoverWrites")
            receipt["writer_blocked"] = state
            receipt["writes_blocked"] = writes
            return (state["dataset"].get("writerBusy") == "true"
                    and h.events(state, "graphshell-edit-admit")
                    and any(w["outcome"] == "pending" and not w["requests"] for w in writes))

        h.wait("Product save queued behind the native transaction", blocked, timeout=6)
        click("Edit here")
        receipt["busy_status"] = h.wait("Handover refused as busy", lambda: browser.evaluate(
            follower, f"document.querySelector('#writer-standby-status').textContent === {json.dumps(BUSY)}"
                      " && document.querySelector('#writer-standby-status').textContent"), timeout=5)
        h.require(locks("locks_after_refusal") == original_client, "Writer lock changed on refusal")
        h.require(blocked(), "Save was no longer pending at the busy refusal")
        state = receipt["writer_blocked"]
        h.require(h.ready(state, "writer", "hidden") and not h.events(state, "graphshell-edit-admitted")
                  and browser.evaluate(writer, "performance.timeOrigin") == writer_document,
                  "Original writer changed or answered while blocked")
        pending = browser.evaluate(follower, h.STATE)
        receipt["follower_after_refusal"] = pending
        h.require(h.ready(pending, "follower", "visible") and not h.events(pending, "graphshell-edit-reply"),
                  "Follower promoted or received a reply while blocked")
        receipt["barrier_at_refusal"] = browser.evaluate(follower, "handoverBarrier")
        h.require(receipt["barrier_at_refusal"]["phase"] == "holding"
                  and not receipt["barrier_at_refusal"].get("expired"), "Barrier expired")
        screenshot("busy-refusal")
    finally:
        browser.evaluate(follower, "(handoverBarrier.release = true)")

    h.wait("Native barrier completed", lambda: browser.evaluate(
        follower, 'handoverBarrier.phase === "complete"'), timeout=5)
    receipt["barrier_after_release"] = browser.evaluate(follower, "handoverBarrier")
    h.require(not receipt["barrier_after_release"].get("expired"), "Barrier reached its safety deadline")
    saved = h.observe(browser, follower, "follower_saved",
                      lambda s: h.events(s, "graphshell-edit-reply")
                      and h.labels(s["semantic"], title), receipt, timeout=20)
    old = h.observe(browser, writer, "writer_saved",
                    lambda s: h.ready(s, "writer", "hidden")
                    and s["dataset"].get("writerBusy") == "false", receipt)
    receipt["writes_saved"] = browser.evaluate(writer, "handoverWrites")
    h.require(any(before["outcome"] == "pending" and not before["requests"]
                  and after["outcome"] == "complete" and "success" in after["requests"]
                  for before, after in zip(receipt["writes_blocked"], receipt["writes_saved"])),
              "The queued product write did not complete")
    submit = h.events(saved, "graphshell-edit-submit")
    admit = h.events(old, "graphshell-edit-admit")
    reply = h.events(saved, "graphshell-edit-reply")
    answered = h.events(old, "graphshell-edit-admitted")
    h.require(len(submit) == len(admit) == len(reply) == len(answered) == 1,
              "Expected one submit, admit, writer answer and follower reply")
    request = submit[0]["detail"]
    result = reply[0]["detail"]
    h.require(request == admit[0]["detail"] and request["edit"] == {
        "kind": "add_node", "address": receipt["address"], "title": title,
    } and result == answered[0]["detail"] and result["id"] == request["id"]
        and result["status"] == "admitted", f"Edit acknowledgement did not match: {result}")
    h.require(h.labels(saved["semantic"], title) == 1, "Follower rendered duplicate objects")
    h.require(locks("locks_after_save") == original_client, "Refused request later acquired writer lock")

    click("Edit here")
    promoted = h.observe(browser, follower, "new_writer",
                         lambda s: h.ready(s, "writer", "visible")
                         and h.labels(s["semantic"], title), receipt)
    h.observe(browser, writer, "old_writer_hidden_follower",
              lambda s: s.get("role") == "follower" and s["visibility"] == "hidden", receipt)
    receipt["document_time_origins"] = {
        "old_writer_before": writer_document,
        "old_writer_after": browser.evaluate(writer, "performance.timeOrigin"),
        "new_writer_before": follower_document,
        "new_writer_after": browser.evaluate(follower, "performance.timeOrigin"),
    }
    origins = receipt["document_time_origins"]
    h.require(origins["new_writer_after"] == follower_document
              and origins["old_writer_after"] != writer_document,
              "Expected the original writer alone to reload")
    new_client = locks("locks_after_handover")
    h.require(new_client != original_client, "Original writer kept the lock")
    messages = browser.evaluate(follower, "handoverFollower.messages")
    receipt["channel_messages"] = messages
    outcomes = [msg["type"] for msg in messages if msg["type"] in ("busy", "released")]
    h.require(outcomes == ["busy", "released"], f"Unexpected handover responses: {outcomes}")
    screenshot("handover-complete")
    # A newly loaded hidden page may mount without a presentation frame.
    browser.activate(writer)
    demoted = h.observe(browser, writer, "old_writer_visible_reread",
                        lambda s: h.ready(s, "follower", "visible")
                        and h.labels(s["semantic"], title), receipt)
    h.require(all(h.labels(s["semantic"], title) == 1 for s in [promoted, demoted]),
              "Handover views disagree on the saved object count")
    h.observe(browser, follower, "new_writer_hidden",
              lambda s: h.ready(s, "writer", "hidden"), receipt)
    h.require(locks("locks_after_focus_change") == new_client, "Focus change moved writer lock")
    (out / "former-writer-reread.png").write_bytes(base64.b64decode(browser.screenshot(writer)))


def qualify(name, package, out, marker, *, helpers):
    """Run a disposable browser/profile and verify the edit after process restart."""
    h = helpers
    out.mkdir()
    app = h.CHROME if name == "chrome" else h.FIREFOX
    token = uuid.uuid4().hex[:10]
    receipt = {
        "browser": name, "browser_app": str(app.parent), "headed": True,
        "platform": platform.platform(), "producer_marker": marker,
        "started_utc": datetime.now(timezone.utc).isoformat(), "package": str(package),
        "title": f"Handover {token}", "address": f"https://qualification.invalid/{token}",
        "result": "fail",
    }
    try:
        with (app / "Info.plist").open("rb") as stream:
            receipt["browser_version"] = plistlib.load(stream)["CFBundleShortVersionString"]
        manifest = json.loads((package / "manifest.json").read_text())
        receipt["extension_version"] = manifest["version"]
        receipt["sha256"] = {
            str(path.relative_to(package)): h.sha256(path)
            for path in sorted(package.rglob("*")) if path.is_file()
        }
        with tempfile.TemporaryDirectory(prefix=f"mere-handover-{name}-") as temp:
            profile = Path(temp) / "profile"
            origin_id = str(uuid.uuid4())

            def launch(phase):
                log = out / f"{phase}.log"
                if name == "chrome":
                    return h.Chrome(profile, log)
                return h.Firefox(profile, log, manifest["browser_specific_settings"]["gecko"]["id"], origin_id)

            with launch("initial") as browser:
                url = browser.install(package) + "/graph.html"
                scenario(browser, url, receipt, out, helpers=h)
            with launch("reopen") as browser:
                reopened_url = browser.install(package) + "/graph.html"
                h.require(url == reopened_url, "Extension origin changed at reopen")
                page = browser.open(reopened_url)
                reopened = h.observe(browser, page, "fresh_process_reopen",
                                     lambda s: h.ready(s, "writer", "visible")
                                     and h.labels(s["semantic"], receipt["title"]), receipt)
                h.require(h.labels(reopened["semantic"], receipt["title"]) == 1,
                          "Reopen rendered duplicate objects")
                (out / "fresh-process-reopen.png").write_bytes(base64.b64decode(browser.screenshot(page)))
        receipt["result"] = "pass"
    except Exception:
        receipt["failure"] = traceback.format_exc()
        print(receipt["failure"], flush=True)
    finally:
        (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"{name}: {receipt['result']} ({out / 'receipt.json'})", flush=True)
    return receipt["result"] == "pass"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--browser", choices=["chrome", "firefox", "both"], default="both")
    parser.add_argument("--marker", type=Path, default=Path("/tmp/mere-extension-ready"))
    parser.add_argument("--chrome-package", type=Path, default=Path("/tmp/mere-extension-chromium"))
    parser.add_argument("--firefox-package", type=Path, default=Path("/tmp/mere-extension-firefox"))
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location(
        "handover_helpers", Path(__file__).with_name("qualify-hidden-writer.py"))
    helpers = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helpers)
    marker = args.marker.read_text().strip()
    args.out.mkdir(parents=True)
    browsers = ["chrome", "firefox"] if args.browser == "both" else [args.browser]
    results = [qualify(name, getattr(args, f"{name}_package").resolve(), args.out / name,
                       marker, helpers=helpers) for name in browsers]
    return 0 if all(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
