#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0
"""Headed extension qualification of the goal surface and durable tag suggestions.

Fixture profile only (a disposable browser profile, five invented objects).
Process 1 seeds the profile through the page's own forms, runs a goal, suggests
tags, accepts one and rejects one. Process 2 reopens the same profile directory
cold and reads the suggestions with their review states and the accepted tag;
a second tab there is a follower and reads the same, read-only.

Usage: qualify-goal-surface.py --out DIR --package PATH [--browser chrome]
Requires Python with websocket-client (the hidden-writer harness imports it).
"""

import argparse
import base64
from datetime import datetime, timezone
import importlib.util
import json
from pathlib import Path
import platform
import plistlib
import shutil
import traceback

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("hidden_writer", HERE / "qualify-hidden-writer.py")
hw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(hw)
wait, require, button, field = hw.wait, hw.require, hw.button, hw.field

# (address, title, tags). The first object's tags define the two concepts the
# suggestions draw on; the others start untagged.
FIXTURE = [
    ("https://notes.example/starter-basics", "Sourdough starter basics: feeding and flour", "sourdough, baking"),
    ("https://notes.example/away", "Keeping a sourdough starter alive while away for a month", ""),
    ("https://notes.example/oven", "Oven steam helps baking loaves rise", ""),
    ("https://notes.example/travel", "Dry a sourdough starter into flakes for travel baking", ""),
    ("https://notes.example/taxes", "Quarterly tax filing checklist", ""),
]
GOAL = "keep a sourdough starter alive while away"
EXPECTED = {  # (title fragment, concept) -> what the lexical producer should propose
    ("Keeping a sourdough", "sourdough"), ("Oven steam", "baking"),
    ("Dry a sourdough", "sourdough"), ("Dry a sourdough", "baking"),
}

TEXTS = """(() => {
  const out = [];
  const walk = (node) => {
    if (!node) return;
    for (const key of ["label", "text", "name", "value"]) {
      if (typeof node[key] === "string" && node[key].trim()) out.push(node[key].trim());
    }
    (node.children || []).forEach(walk);
  };
  walk(globalThis.graphshellSemanticTree?.());
  return out;
})()"""


def texts(browser, page):
    return browser.evaluate(page, TEXTS) or []


def has_text(browser, page, needle):
    return any(needle in line for line in texts(browser, page))


def shot(browser, page, out, name, receipt):
    path = out / f"{name}.png"
    path.write_bytes(base64.b64decode(browser.screenshot(page)))
    receipt.setdefault("screenshots", []).append(str(path))


def dump_on_timeout(browser, page, what):
    try:
        print(f"--- timed out: {what}; page text:", flush=True)
        for line in texts(browser, page):
            print("   ", line[:160], flush=True)
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the failure
        print("   (could not read the page)", error, flush=True)


def press(label):
    """Like the harness's button(), but also accepts a label the mirror repeats up to three times."""
    return f"""(() => {{
      const label = {json.dumps(label)};
      const node = [...document.querySelectorAll('button, [role="button"]')].find(node => {{
        const text = node.getAttribute("aria-label") || node.textContent.trim();
        return [1, 2, 3].some(n => text === label.repeat(n)) && node.getClientRects().length;
      }});
      if (!node || node.disabled) return false;
      node.click();
      return true;
    }})()"""


def has_field(browser, page, label):
    return browser.evaluate(page, f"!!document.querySelector('[role=\"textbox\"][aria-label=\"{label}\"]')")


def strip_click(browser, page, label, expect):
    for _ in range(4):
        real_click(browser, page, label)
        for _ in range(8):
            hw.time.sleep(0.5)
            if expect():
                return
    raise TimeoutError(f"click {label}")


def click(browser, page, label, expect=None, timeout=40):
    """Press a control, then require its effect (`expect`: text shown or a field label),
    pressing again if the page did not take the first click."""
    def seen():
        if expect is None:
            return True
        if callable(expect):
            return expect()
        return has_text(browser, page, expect)

    if expect is not None and seen() and label not in ("Panes", "Inspector"):
        return
    deadline = hw.time.monotonic() + timeout
    while hw.time.monotonic() < deadline:
        if browser.evaluate(page, press(label)):
            for _ in range(10):
                hw.time.sleep(0.4)
                if seen():
                    return
        else:
            hw.time.sleep(0.4)
    dump_on_timeout(browser, page, f"click {label}")
    raise TimeoutError(f"click {label}")


RECT_JS = """(() => {{
  const node = document.querySelector('[role="textbox"][aria-label="{label}"]');
  if (!node) return null;
  node.scrollIntoView({{block: "center"}});
  const r = node.getBoundingClientRect();
  return r.width && r.height ? {{x: r.x + Math.min(r.width / 2, 40), y: r.y + r.height / 2}} : null;
}})()"""


def node_rect(label):
    return f"""(() => {{
      const label = {json.dumps(label)};
      const node = [...document.querySelectorAll('button, [role="button"], [role="textbox"]')].find(n => {{
        const text = n.getAttribute("aria-label") || n.textContent.trim();
        return [1, 2, 3].some(k => text === label.repeat(k)) && n.getClientRects().length;
      }});
      if (!node) return null;
      const r = node.getBoundingClientRect();
      return {{x: r.x + Math.min(r.width / 2, 40), y: r.y + r.height / 2}};
    }})()"""


def mouse(browser, page, x, y):
    for kind, buttons in (("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 0)):
        browser.call("Input.dispatchMouseEvent", {
            "type": kind, "x": x, "y": y, "button": "left" if buttons else "none",
            "buttons": buttons, "clickCount": 1}, sessionId=page)


REGIONS = {"strip": (300, 150, 75, 175), "dock": (1050, 500, 140, 860)}


def bring_into_view(browser, page, label, region="strip", anchor=None):
    """Scroll the clipped document-tools strip with the wheel until the control is inside it."""
    wx, wy, top, bottom = REGIONS[region]
    for _ in range(40):
        rect = browser.evaluate(page, node_rect(label))
        if rect and (anchor is None and top <= rect["y"] <= bottom or anchor is not None and abs(rect["y"] - anchor) < 40):
            return rect
        target = anchor if anchor is not None else (top + bottom) / 2
        delta = max(-900, min(900, rect["y"] - target)) if rect else 0
        browser.call("Input.dispatchMouseEvent", {"type": "mouseMoved", "x": wx, "y": wy}, sessionId=page)
        browser.call("Input.dispatchMouseEvent", {"type": "mouseWheel", "x": wx, "y": wy,
                                                  "deltaX": 0, "deltaY": delta}, sessionId=page)
        hw.time.sleep(0.4)
    raise TimeoutError(f"could not scroll {label} into view")


def real_click(browser, page, label, region="strip"):
    browser.activate(page)
    rect = bring_into_view(browser, page, label, region)
    mouse(browser, page, rect["x"], rect["y"])


def key(browser, page, kind, **fields):
    browser.call("Input.dispatchKeyEvent", {"type": kind, **fields}, sessionId=page)


def type_into(browser, page, label, value, timeout=20, tab=False, dom_focus=False, region="strip"):
    """Click the field with a real mouse click (or, `tab`, Tab to it from the focused field,
    for a field the canvas clips out of reach) and type with real key events."""
    selector = json.dumps(f'[role="textbox"][aria-label="{label}"]')
    read = f"document.querySelector({selector})?.textContent"
    for attempt in range(3):
        browser.activate(page)
        real_click(browser, page, label, region)
        hw.time.sleep(0.4)
        if browser.evaluate(page, read):
            key(browser, page, "keyDown", key="a", code="KeyA", modifiers=4, commands=["selectAll"])
            key(browser, page, "keyUp", key="a", code="KeyA", modifiers=4)
            key(browser, page, "keyDown", key="Backspace", code="Backspace", windowsVirtualKeyCode=8)
            key(browser, page, "keyUp", key="Backspace", code="Backspace", windowsVirtualKeyCode=8)
        for ch in value:
            key(browser, page, "keyDown", key=ch, text=ch, unmodifiedText=ch)
            key(browser, page, "keyUp", key=ch)
        try:
            wait(f"confirm {label}", lambda: browser.evaluate(page, read) == value, timeout=6)
            return
        except TimeoutError:
            print(f"retry {label}: field holds {browser.evaluate(page, read)!r}", flush=True)
    raise TimeoutError(f"{label} never held {value!r}")


def open_tools(browser, page):
    if not has_text(browser, page, "Find in what I kept"):
        click(browser, page, "Panes", expect="Peek selection")
        click(browser, page, "Inspector", expect="Find in what I kept")


def dock_click(browser, page, label, expect=None):
    """A real mouse click inside the docked Graph tools, scrolled into view first."""
    for _ in range(4):
        real_click(browser, page, label, "dock")
        if expect is None:
            return
        for _ in range(10):
            hw.time.sleep(0.4)
            if expect():
                return
    raise TimeoutError(f"click {label}")


def frame(browser, page, label, at=130):
    """Scroll the dock so `label` sits near the top, to frame what follows it."""
    bring_into_view(browser, page, label, "dock", anchor=at)
    hw.time.sleep(0.6)


def loaded(browser, page):
    return any(line.startswith("Read ") and "retained records" in line for line in texts(browser, page))


def open_goal(browser, page):
    open_tools(browser, page)
    if not has_text(browser, page, "Read retained records again"):
        dock_click(browser, page, "Find in what I kept",
                   lambda: has_text(browser, page, "Read retained records again"))
    wait("goal panel loaded", lambda: loaded(browser, page), 40)


def settle(browser, page):
    """Wait for the writer to finish any save, then a beat for the next frame."""
    wait("writer idle", lambda: browser.evaluate(
        page, "document.querySelector('graphshell-view,graphshell-tree')?.dataset.writerBusy === 'false'"), 40)
    hw.time.sleep(0.8)


def show_details(browser, page, title):
    """Select the object by its canvas node and read its details, opening them if closed."""
    wait("select object", lambda: browser.evaluate(page, hw.select_object(title)), timeout=20)
    deadline_ok = False
    for _ in range(60):
        if browser.evaluate(page, hw.detail_field("Title")) == title:
            deadline_ok = True
            break
        browser.evaluate(page, button("Open details"))
        hw.time.sleep(0.5)
    require(deadline_ok, f"details for {title} did not open")


def seed(browser, writer, receipt):
    click(browser, writer, "Panes", expect="Document tools")
    click(browser, writer, "Document tools", expect="Add an object")
    for address, title, tags in FIXTURE:
        settle(browser, writer)
        strip_click(browser, writer, "Add an object", lambda: has_field(browser, writer, "Address"))
        type_into(browser, writer, "Address", address)
        type_into(browser, writer, "New object title", title)
        strip_click(browser, writer, "New node", lambda: hw.labels(
            browser.evaluate(writer, hw.STATE)["semantic"], title) > 0)
        wait(f"{title} added", lambda: hw.labels(browser.evaluate(writer, hw.STATE)["semantic"], title) > 0, 40)
        wait("new object's details", lambda: browser.evaluate(writer, hw.detail_field("Title")) == title, 30)
        if tags:
            type_into(browser, writer, "Tags", tags)
            strip_click(browser, writer, "Save changes", lambda: True)
            # The saved tags come back sorted; seeing them is the save finishing.
            saved = ", ".join(sorted(t.strip() for t in tags.split(",")))
            hw.time.sleep(1.0)
            settle(browser, writer)
    receipt["seeded"] = [title for _, title, _ in FIXTURE]


def suggestion_lines(browser, page):
    return [line for line in texts(browser, page) if line.startswith("Suggestion · ")]


def review_of(line):
    return line.split(" · ")[3]


def process_one(browser, url, out, receipt):
    writer = browser.open(url)
    hw.observe(browser, writer, "writer_initial", lambda s: hw.ready(s, "writer", "visible"), receipt)
    seed(browser, writer, receipt)
    shot(browser, writer, out, "01-seeded-graph", receipt)

    open_goal(browser, writer)
    receipt["status_after_load"] = [t for t in texts(browser, writer) if t.startswith("Read ")]
    type_into(browser, writer, "Goal", GOAL, region="dock")
    dock_click(browser, writer, "Find", lambda: has_text(browser, writer, "sources read matched"))
    receipt["goal_summary"] = [t for t in texts(browser, writer) if "sources read matched" in t]
    frame(browser, writer, "Find")
    shot(browser, writer, out, "02-goal-run", receipt)

    dock_click(browser, writer, "Suggest tags for all retained pages",
               lambda: len(suggestion_lines(browser, writer)) >= len(EXPECTED))
    lines = suggestion_lines(browser, writer)
    receipt["suggestions_before"] = lines
    found = {(t, c) for t, c in EXPECTED if any(f" · {c} · {t}" in line for line in lines)}
    require(found == EXPECTED and len(lines) == len(EXPECTED), f"unexpected suggestions: {lines}")
    require(all(review_of(line) == "Not reviewed" for line in lines), "a new suggestion was already reviewed")
    frame(browser, writer, "Suggest tags for all retained pages", at=110)
    shot(browser, writer, out, "03-suggestions", receipt)

    accept = "Accept sourdough for Keeping a sourdough starter alive while away for a month"
    dock_click(browser, writer, accept, lambda: any(
        review_of(l).startswith("Accepted") for l in suggestion_lines(browser, writer)))
    reject = "Reject baking for Oven steam helps baking loaves rise"
    dock_click(browser, writer, reject, lambda: any(
        review_of(l).startswith("Rejected") for l in suggestion_lines(browser, writer)))
    receipt["suggestions_after_decisions"] = suggestion_lines(browser, writer)
    receipt["notes"] = [t for t in texts(browser, writer) if t.startswith(("Rejected", "Accepted", "Tag added"))]
    frame(browser, writer, "Suggest tags for all retained pages", at=110)
    shot(browser, writer, out, "04-accepted-and-rejected", receipt)

    # Regenerating proposes the same concepts again under a new run. The tag the
    # person accepted is held now, so it is not proposed again; the earlier
    # suggestions keep their review states.
    receipt["b_sourdough_rows_before_regenerate"] = b_rows(suggestion_lines(browser, writer))
    settle(browser, writer)
    dock_click(browser, writer, "Suggest tags for all retained pages",
               lambda: len(suggestion_lines(browser, writer)) == 7)
    lines = suggestion_lines(browser, writer)
    receipt["suggestions_after_regenerate"] = lines
    require(b_rows(lines) == 1, "the accepted tag was proposed again, so it is not held")
    states = sorted(review_of(l).split(" by ")[0] for l in lines)
    require(states == ["Accepted", "Not reviewed", "Not reviewed", "Not reviewed", "Not reviewed",
                       "Not reviewed", "Rejected"], f"regenerating changed review states: {states}")
    frame(browser, writer, "Suggest tags for all retained pages", at=110)
    shot(browser, writer, out, "05-regenerated", receipt)


def b_rows(lines):
    return sum(" · sourdough · Keeping a sourdough" in line for line in lines)


def process_two(browser, url, out, receipt):
    writer = browser.open(url)
    hw.observe(browser, writer, "reopen_writer", lambda s: hw.ready(s, "writer", "visible")
               and hw.labels(s["semantic"], FIXTURE[1][1]), receipt, 60)
    open_goal(browser, writer)
    wait("reopened suggestions", lambda: len(suggestion_lines(browser, writer)) == 7, 40)
    lines = suggestion_lines(browser, writer)
    receipt["reopen_suggestions"] = lines
    kinds = sorted(review_of(l).split(" by ")[0] for l in lines)
    require(kinds == ["Accepted", "Not reviewed", "Not reviewed", "Not reviewed", "Not reviewed",
                      "Not reviewed", "Rejected"], f"review states lost: {lines}")
    require(any(review_of(l).startswith("Accepted") and "Keeping a sourdough" in l for l in lines),
            "the accepted suggestion is not the one accepted")
    require(any(review_of(l).startswith("Rejected") and "Oven steam" in l for l in lines),
            "the rejected suggestion is not the one rejected")
    # The accepted tag is held after the reopen: regenerating does not propose it again.
    settle(browser, writer)
    dock_click(browser, writer, "Suggest tags for all retained pages",
               lambda: len(suggestion_lines(browser, writer)) == 10)
    after = suggestion_lines(browser, writer)
    receipt["reopen_suggestions_after_regenerate"] = after
    require(b_rows(after) == 1, "the accepted tag is not held after a cold reopen")
    frame(browser, writer, "Suggest tags for all retained pages", at=110)
    shot(browser, writer, out, "05-cold-reopen", receipt)

    follower = browser.open(url)
    hw.observe(browser, follower, "follower_initial", lambda s: hw.ready(s, "follower", "visible"), receipt, 60)
    open_goal(browser, follower)
    wait("follower suggestions", lambda: len(suggestion_lines(browser, follower)) == 10, 40)
    receipt["follower_suggestions"] = suggestion_lines(browser, follower)
    receipt["follower_write_controls"] = [t for t in texts(browser, follower)
                                          if t.startswith(("Accept ", "Reject ", "Suggest tags"))]
    require(not receipt["follower_write_controls"], f"follower offered writes: {receipt['follower_write_controls']}")
    type_into(browser, follower, "Goal", GOAL, region="dock")
    dock_click(browser, follower, "Find", lambda: has_text(browser, follower, "sources read matched"))
    receipt["follower_goal_summary"] = [t for t in texts(browser, follower) if "sources read matched" in t]
    frame(browser, follower, "Find")
    shot(browser, follower, out, "06-follower-goal", receipt)
    bring_into_view(browser, follower, "Read retained records again", "dock", anchor=500)
    shot(browser, follower, out, "07-follower-suggestions", receipt)


def qualify(package, out):
    app = hw.CHROME
    out.mkdir(parents=True)
    receipt = {
        "browser": "chrome", "browser_app": str(app.parent), "headed": True, "fixture_profile": True,
        "platform": platform.platform(), "scenario": "goal-surface-tag-suggestions-cold-reopen",
        "started_utc": datetime.now(timezone.utc).isoformat(), "package": str(package), "result": "fail",
        "goal": GOAL,
    }
    profile = out / "profile"
    try:
        with (app / "Info.plist").open("rb") as stream:
            receipt["browser_version"] = plistlib.load(stream)["CFBundleShortVersionString"]
        receipt["extension_version"] = json.loads((package / "manifest.json").read_text())["version"]
        with hw.Chrome(profile, out / "initial.log") as browser:
            url = browser.install(package) + "/graph.html"
            process_one(browser, url, out, receipt)
        with hw.Chrome(profile, out / "reopen.log") as browser:
            require(browser.install(package) + "/graph.html" == url, "extension origin changed")
            process_two(browser, url, out, receipt)
        receipt["result"] = "pass"
    except Exception:
        receipt["failure"] = traceback.format_exc()
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
    args = parser.parse_args()
    return 0 if qualify(args.package.resolve(), args.out) else 1


if __name__ == "__main__":
    raise SystemExit(main())
