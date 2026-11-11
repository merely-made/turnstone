#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Headed Chrome qualification of "Keep page text" on a fixture profile.

A local test server (127.0.0.1) serves invented pages. Live remembering stays
OFF throughout: the three graph objects are seeded through the page's own forms,
so the run also shows the two controls are separate. Process 1: a goal that only
page bodies match finds nothing; the person turns on **Keep page text** (the
browser's optional-access prompt is answered by the operator or, with
--auto-grant, by the macOS accessibility API); pages are loaded in tabs; the
panel shows kept/skipped/cut counts; the same goal now cites body passages; a
reload of an unchanged page is "unchanged". Process 2 reopens the profile cold
and runs the goal again. The server log proves no page was fetched except the
ones the run loaded (the pages carry a link that must never be requested).

Usage: qualify-keep-content.py --out DIR --package PATH [--auto-grant]
Requires Python with websocket-client.
"""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import threading
import traceback
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = Path(__file__).resolve().parent


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gs = load("goal_surface_harness", "qualify-goal-surface.py")
hw = gs.hw
wait, require = hw.wait, hw.require

FILLER = ("Compost notes continue with moisture, aeration and patience. " * 14)
PAGES = {
    "/notes/1": ("Garden notes one", """<nav>NAVJUNKWORD home about</nav><main><h1>Garden notes</h1>
        <p>Vermicast is worm castings. A damp, warm bin lets red wigglers turn kitchen scraps into vermicast
        within weeks, and the finished vermicast feeds seedlings without burning them. {pad}</p>
        <p style="display:none">HIDDENPARAGRAPH</p><script>var SCRIPTJUNK = 1;</script>
        <a href="/never/visited">a link nobody follows</a></main><footer>FOOTERJUNK</footer>"""),
    "/notes/2": ("Garden notes two", """<main><h1>Bins</h1><p>Wriggler bins need shade. Bedding of shredded cardboard
        holds moisture; fruit rinds attract fungus gnats, so bury them. Tanglewort grows nearby. {pad}</p>
        <a href="/never/visited">another link</a></main>"""),
    "/notes/3": ("Garden notes three", "<main><h1>Long</h1><p>QUILLBACK begins this long page. " + "{pad} " * 60
                 + "ZZFINALMARKER ends it.</p></main>"),
    "/secret": ("Excluded page", "<main><h1>Secret</h1><p>Tanglefoot is only on the excluded origin. {pad}</p></main>"),
}
PAD = FILLER.strip()
GOAL = "vermicast red wigglers seedlings"
EXCLUDED_GOAL = "tanglefoot excluded origin"
LONG_GOAL = "quillback"
CUT_GOAL = "zzfinalmarker"
SEEDS = [  # (path, title): titles and addresses hold none of the goal's words
    ("/notes/1", "Garden notes one"), ("/notes/2", "Garden notes two"), ("/notes/3", "Garden notes three"),
]


class Site:
    """Invented pages on 127.0.0.1, and a log of every request made to them."""

    def __init__(self):
        self.requests = []
        site = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):  # noqa: N802 - http.server API
                site.requests.append(self.path)
                if self.path in PAGES:
                    title, body = PAGES[self.path]
                    data = (f"<!doctype html><html><head><meta charset=utf-8><title>{title}</title></head>"
                            f"<body>{body.replace('{pad}', PAD)}</body></html>").encode()
                    kind = "text/html; charset=utf-8"
                elif self.path == "/notes/raw.txt":
                    data, kind = b"plain text, not a web page: rawtextword", "text/plain"
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header("Content-Type", kind)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *_):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.port = self.server.server_address[1]
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def body_text(self, path):
        return PAGES[path][1]


def shot(browser, page, out, name, receipt):
    gs.shot(browser, page, out, name, receipt)


def panel_text(browser, page):
    return browser.evaluate(page, """(() => {
      const text = (id) => document.querySelector(id)?.textContent ?? "";
      return { status: text("#keep-status"), storage: text("#keep-storage"), problem: text("#keep-problem"),
        enable: document.querySelector("#keep-enable")?.disabled, disable: document.querySelector("#keep-disable")?.disabled };
    })()""")


def open_panel(browser, page):
    browser.evaluate(page, """(() => { const d = document.querySelector("#keep-panel"); d.open = true;
      d.scrollIntoView({block: "center"}); })()""")
    hw.time.sleep(0.5)


def real_click_dom(browser, page, selector):
    """Click with a user activation (DevTools `userGesture`), which the browser's access request needs."""
    browser.activate(page)
    browser.call("Runtime.evaluate", {
        "expression": f"document.querySelector({json.dumps(selector)}).click()", "userGesture": True,
        "returnByValue": True}, sessionId=page)


def grant_prompt():
    """Press Allow on the browser's own permission bubble through macOS accessibility."""
    script = '''
tell application "System Events"
  tell process "Google Chrome"
    repeat 40 times
      repeat with w in windows
        try
          set b to first button of (entire contents of w) whose name is "Allow"
          click b
          return "clicked"
        end try
      end repeat
      delay 0.5
    end repeat
  end tell
end tell
return "no prompt found"'''
    result = subprocess.run(["osascript", "-e", script], capture_output=True, text=True, timeout=60)
    return (result.stdout + result.stderr).strip()


def goal_run(browser, page, goal, receipt, key, out=None, name=None, wait_hits=True):
    gs.type_into(browser, page, "Goal", goal, region="dock")
    for attempt in range(4):
        gs.dock_click(browser, page, "Find", lambda: gs.has_text(browser, page, "sources read matched"))
        hw.time.sleep(0.8)
        summary = [t for t in gs.texts(browser, page) if "sources read matched" in t]
        if summary and goal in summary[0]:
            break
    lines = gs.texts(browser, page)
    receipt[key] = {"summary": summary, "cited": [t for t in lines if " · version " in t],
                    "passages": [t for t in lines if "Matched " not in t and any(w in t.lower() for w in goal.split())][:12]}
    if name:
        gs.frame(browser, page, "Find")
        shot(browser, page, out, name, receipt)
    return receipt[key]


def matched_count(run):
    return int(run["summary"][0].split(" of ")[0]) if run["summary"] else 0


def reread(browser, page):
    gs.dock_click(browser, page, "Read retained records again", lambda: True)
    hw.time.sleep(1.5)
    gs.wait("records read", lambda: gs.loaded(browser, page), 40)


def idb_keys(browser, page):
    """Every key the page's IndexedDB holds, to show the PageTextStore key layout."""
    result = browser.call("Runtime.evaluate", {"awaitPromise": True, "returnByValue": True, "expression": """(async () => {
      const out = {};
      for (const info of await indexedDB.databases()) {
        const db = await new Promise((res, rej) => { const r = indexedDB.open(info.name); r.onsuccess = () => res(r.result); r.onerror = () => rej(r.error); });
        for (const name of db.objectStoreNames) {
          const keys = await new Promise((res, rej) => { const r = db.transaction(name).objectStore(name).getAllKeys(); r.onsuccess = () => res(r.result); r.onerror = () => rej(r.error); });
          out[info.name + "/" + name] = keys.map(String);
        }
        db.close();
      }
      return JSON.stringify(out);
    })()"""}, sessionId=page)
    return result["result"]["value"]


def load_page(browser, url):
    tab = browser.open(url)
    hw.time.sleep(2.0)
    return tab


def process_one(browser, site, url, out, receipt, auto_grant):
    base = f"http://127.0.0.1:{site.port}"
    gs.FIXTURE = [(base + path, title, "") for path, title in SEEDS]
    writer = browser.open(url)
    hw.observe(browser, writer, "writer_initial", lambda s: hw.ready(s, "writer", "visible"), receipt)
    receipt["panel_first"] = (lambda: (open_panel(browser, writer), panel_text(browser, writer))[1])()
    require("Page text is off" in receipt["panel_first"]["status"], f"not off by default: {receipt['panel_first']}")
    require(receipt["panel_first"]["enable"] is False, "Keep page text should be offerable")
    shot(browser, writer, out, "01-control-off-by-default", receipt)

    # The exclusion is set under Browser history before anything is kept.
    browser.evaluate(writer, f"""(() => {{ const d = document.querySelector("#capture-panel"); d.open = true;
      document.querySelector("#capture-exclusions").value = "http://localhost:{site.port}"; }})()""")
    real_click_dom(browser, writer, "#capture-save")
    hw.time.sleep(1.0)

    gs.seed(browser, writer, receipt)
    gs.open_goal(browser, writer)
    before = goal_run(browser, writer, GOAL, receipt, "goal_before_keeping", out, "02-goal-before-titles-only")
    require(matched_count(before) == 0, f"body words matched before anything was kept: {before}")

    # Turn it on. The browser asks for scripting and access to sites.
    open_panel(browser, writer)
    helper = None
    if auto_grant:
        helper = threading.Thread(target=lambda: receipt.__setitem__("grant_helper", grant_prompt()), daemon=True)
        helper.start()
    browser.evaluate(writer, "document.querySelector('#keep-size').value = '32768'")
    real_click_dom(browser, writer, "#keep-enable")
    if not auto_grant:
        print("Answer the browser's access prompt (Allow) within 120 s.", flush=True)
    try:
        wait("keep enabled", lambda: browser.evaluate(writer, "document.querySelector('#keep-status')?.textContent")
             .startswith(("Keeping page text", "Access to the pages")), timeout=120 if not auto_grant else 60)
    except TimeoutError:
        receipt["panel_at_timeout"] = panel_text(browser, writer)
        shot(browser, writer, out, "99-enable-timeout", receipt)
        raise
    panel = panel_text(browser, writer)
    receipt["panel_after_enable"] = panel
    shot(browser, writer, out, "03-control-on", receipt)
    require(panel["status"].startswith("Keeping page text"), f"access not granted: {panel}")
    # The page reloaded to hand over the new setting; wait for the writer again.
    hw.observe(browser, writer, "writer_after_enable", lambda s: hw.ready(s, "writer", "visible"), receipt)

    # Load pages the way a person does: in tabs. One is on the excluded origin,
    # one is not HTML, and /notes/1 is loaded twice (an unchanged page).
    loads = [f"{base}/notes/1", f"{base}/notes/2", f"{base}/notes/3",
             f"http://localhost:{site.port}/secret", f"{base}/notes/raw.txt"]
    tabs = [load_page(browser, address) for address in loads]
    # The writer view drains the queue while it is the visible tab (a hidden page's frames are
    # throttled); text loaded meanwhile waits in the queue, which the panel counts.
    browser.activate(writer)
    open_panel(browser, writer)

    def kept_state():
        text = panel_text(browser, writer)["status"]
        return text if " 3 kept" in text else None
    try:
        receipt["panel_after_loads"] = wait("three pages kept", kept_state, 60)
    except TimeoutError:
        result = browser.call("Runtime.evaluate", {"awaitPromise": True, "returnByValue": True, "expression":
            """(async () => JSON.stringify({status: await chrome.runtime.sendMessage({type: "keep.status"}),
            root: {...document.querySelector("graphshell-view, graphshell-tree").dataset}, panel: document.querySelector("#keep-status").textContent,
            errors: globalThis.graphshellErrors || [], gate: globalThis.graphshellGateFailures || [],
            initial: globalThis.graphshellInitialKeptJson, role: globalThis.graphshellViewRole, trace: globalThis.__t,
            pull: await chrome.runtime.sendMessage({type: "keep.pull"})}))()"""},
            sessionId=writer)
        receipt["debug_at_timeout"] = result["result"].get("value")
        shot(browser, writer, out, "98-kept-timeout", receipt)
        raise
    receipt["panel_after_loads_detail"] = panel_text(browser, writer)
    shot(browser, writer, out, "04-pages-kept", receipt)

    load_page(browser, f"{base}/notes/1")
    browser.activate(writer)
    wait("unchanged counted", lambda: " 1 unchanged" in panel_text(browser, writer)["status"], 40)
    receipt["panel_after_reload"] = panel_text(browser, writer)
    shot(browser, writer, out, "05-unchanged-page-counted", receipt)

    # The keys the browser holds are PageTextStore's and ResourceCaptureStore's.
    keys = json.loads(idb_keys(browser, writer))
    flat = sorted(key for values in keys.values() for key in values)
    receipt["idb_key_prefixes"] = {
        prefix: sum(key.startswith(prefix) for key in flat) for prefix in ("page-text/", "resource-capture/", "blob/")}
    require(receipt["idb_key_prefixes"]["page-text/"] == 3, receipt["idb_key_prefixes"])
    require(receipt["idb_key_prefixes"]["resource-capture/"] == 3, receipt["idb_key_prefixes"])

    gs.open_goal(browser, writer)
    after = goal_run(browser, writer, GOAL, receipt, "goal_after_keeping", out, "06-goal-cites-page-bodies")
    require(matched_count(after) >= 1, f"no body match after keeping: {after}")
    require(any("/notes/1" in line for line in after["cited"]), f"notes/1 not cited: {after['cited']}")
    require(any("vermicast" in p.lower() for p in after["passages"]), f"no body passage: {after['passages']}")
    excluded = goal_run(browser, writer, EXCLUDED_GOAL, receipt, "goal_excluded_origin")
    require(matched_count(excluded) == 0, "the excluded origin's text was kept")
    quill = goal_run(browser, writer, LONG_GOAL, receipt, "goal_long_page_start")
    require(any("/notes/3" in line for line in quill["cited"]), "the start of the long page was not kept")
    cut = goal_run(browser, writer, CUT_GOAL, receipt, "goal_long_page_end_cut", out, "07-long-page-cut-at-limit")
    require(matched_count(cut) == 0, "text past the size limit was kept")
    receipt["server_requests"] = list(site.requests)


def process_two(browser, site, url, out, receipt):
    writer = browser.open(url)
    hw.observe(browser, writer, "reopen_writer", lambda s: hw.ready(s, "writer", "visible")
               and hw.labels(s["semantic"], SEEDS[0][1]), receipt, 60)
    open_panel(browser, writer)
    receipt["panel_after_reopen"] = panel_text(browser, writer)
    gs.open_goal(browser, writer)
    again = goal_run(browser, writer, GOAL, receipt, "goal_after_cold_reopen", out, "08-cold-reopen-goal")
    require(matched_count(again) >= 1 and any("/notes/1" in line for line in again["cited"]),
            f"cold reopen lost the kept text: {again}")
    require(any("vermicast" in p.lower() for p in again["passages"]), "cold reopen cites no body passage")
    open_panel(browser, writer)
    shot(browser, writer, out, "09-cold-reopen-panel", receipt)


def qualify(package, out, auto_grant):
    out.mkdir(parents=True)
    site = Site()
    receipt = {
        "browser": "chrome", "headed": True, "fixture_profile": True, "platform": platform.platform(),
        "scenario": "keep-page-text-goal-cold-reopen", "started_utc": datetime.now(timezone.utc).isoformat(),
        "package": str(package), "result": "fail", "live_remembering": "off throughout",
        "goal": GOAL, "server": f"127.0.0.1:{site.port}",
    }
    profile = out / "profile"
    try:
        with (hw.CHROME / "Info.plist").open("rb") as stream:
            receipt["browser_version"] = plistlib.load(stream)["CFBundleShortVersionString"]
        receipt["extension_version"] = json.loads((package / "manifest.json").read_text())["version"]
        with hw.Chrome(profile, out / "initial.log") as browser:
            url = browser.install(package) + "/graph.html"
            process_one(browser, site, url, out, receipt, auto_grant)
        with hw.Chrome(profile, out / "reopen.log") as browser:
            require(browser.install(package) + "/graph.html" == url, "extension origin changed")
            process_two(browser, site, url, out, receipt)
        # Nothing was fetched but the pages the run loaded: the pages' links were never requested.
        receipt["requests_after_everything"] = list(site.requests)
        require("/never/visited" not in site.requests, "a destination was fetched")
        expected = {"/notes/1", "/notes/2", "/notes/3", "/secret", "/notes/raw.txt", "/favicon.ico"}  # the favicon is the browser's own
        require(set(site.requests) <= expected, f"unexpected requests {site.requests}")
        receipt["result"] = "pass"
    except Exception:
        receipt["failure"] = traceback.format_exc()
        receipt["server_requests_at_failure"] = list(site.requests)
        print(receipt["failure"], flush=True)
    finally:
        (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        shutil.rmtree(profile, ignore_errors=True)
    print(f"chrome: {receipt['result']} ({out / 'receipt.json'})", flush=True)
    return receipt["result"] == "pass"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--auto-grant", action="store_true")
    args = parser.parse_args()
    return 0 if qualify(args.package.resolve(), args.out, args.auto_grant) else 1


if __name__ == "__main__":
    raise SystemExit(main())
