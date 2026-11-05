#!/usr/bin/env python3
# Copyright 2026 Mark Alan Boykin
# SPDX-License-Identifier: MPL-2.0
"""Headed macOS extension qualification. Requires Python and websocket-client.

Run with a new --out directory. Waits for the package producer's marker,
then uses disposable browser profiles and retains JSON receipts and logs.
"""

import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import select
import signal
import subprocess
import tempfile
import time
import traceback
import uuid

import websocket


CHROME = Path("/Applications/Google Chrome.app/Contents")
FIREFOX = Path("/Applications/Firefox.app/Contents")


def wait(label, read, timeout=60):
    deadline = time.monotonic() + timeout
    while True:
        value = read()
        if value:
            return value
        if time.monotonic() >= deadline:
            raise TimeoutError(label)
        time.sleep(0.2)


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class Protocol:
    sequence = 0

    def call(self, method, params=None, **fields):
        self.sequence += 1
        request = dict(id=self.sequence, method=method, params=params or {}, **fields)
        self.send(json.dumps(request))
        deadline = time.monotonic() + 40
        while time.monotonic() < deadline:
            reply = json.loads(self.receive(max(0.01, deadline - time.monotonic())))
            if reply.get("id") == request["id"]:
                if "error" in reply:
                    raise RuntimeError(f"{method}: {reply}")
                return reply.get("result", {})
        raise TimeoutError(method)


class Chrome(Protocol):
    def __init__(self, profile, log):
        self.profile = profile
        self.log = log

    def __enter__(self):
        # Chromium reads on fd 3 and writes on fd 4. Duplicate the source
        # descriptors above that range before constructing the spawn actions.
        import fcntl

        request, self.writer = os.pipe()
        self.reader, response = os.pipe()
        source = fcntl.fcntl(request, fcntl.F_DUPFD, 10)
        sink = fcntl.fcntl(response, fcntl.F_DUPFD, 10)
        binary = str(CHROME / "MacOS/Google Chrome")
        try:
            with self.log.open("wb") as log:
                actions = [(os.POSIX_SPAWN_DUP2, fd, dest)
                           for fd, dest in [(source, 3), (sink, 4), (log.fileno(), 1), (log.fileno(), 2)]]
                # The log descriptor can itself be 3 or 4.
                actions = actions[2:] + actions[:2]
                self.pid = os.posix_spawn(binary, [
                    binary, f"--user-data-dir={self.profile}", "--remote-debugging-pipe",
                    "--enable-unsafe-extension-debugging", "--no-first-run",
                    "--no-default-browser-check", "about:blank",
                ], os.environ, file_actions=actions)
        except BaseException:
            os.close(self.writer)
            os.close(self.reader)
            raise
        finally:
            for fd in [request, response, source, sink]:
                os.close(fd)
        self.buffer = bytearray()
        return self

    def send(self, message):
        data = memoryview(message.encode() + b"\0")
        while data:
            data = data[os.write(self.writer, data):]

    def receive(self, timeout):
        deadline = time.monotonic() + timeout
        while b"\0" not in self.buffer:
            if not select.select([self.reader], [], [], max(0, deadline - time.monotonic()))[0]:
                raise TimeoutError("Chrome DevTools pipe")
            data = os.read(self.reader, 65536)
            require(data, "Chrome closed its DevTools pipe")
            self.buffer.extend(data)
        message, self.buffer = self.buffer.split(b"\0", 1)
        return message

    def __exit__(self, *_):
        try:
            self.call("Browser.close")
        except (OSError, RuntimeError):
            pass
        finally:
            os.close(self.writer)
            os.close(self.reader)
            deadline = time.monotonic() + 15
            while not os.waitpid(self.pid, os.WNOHANG)[0]:
                if time.monotonic() > deadline:
                    os.kill(self.pid, signal.SIGKILL)
                    os.waitpid(self.pid, 0)
                    raise RuntimeError("Chrome did not shut down cleanly")
                time.sleep(0.1)

    def install(self, package):
        return "chrome-extension://" + self.call("Extensions.loadUnpacked", {"path": str(package)})["id"]

    def open(self, url):
        target = self.call("Target.createTarget", {"url": url})["targetId"]
        session = self.call("Target.attachToTarget", {"targetId": target, "flatten": True})["sessionId"]
        self.call("Target.activateTarget", {"targetId": target})
        return session

    def evaluate(self, page, expression):
        result = self.call("Runtime.evaluate", {
            "expression": expression, "returnByValue": True,
        }, sessionId=page)
        require("exceptionDetails" not in result, f"Chrome script error: {result}")
        return result["result"].get("value")

    def screenshot(self, page):
        return self.call("Page.captureScreenshot", sessionId=page)["data"]


class Firefox(Protocol):
    def __init__(self, profile, log, extension_id, origin_id):
        self.profile = profile
        self.log = log
        self.extension_id = extension_id
        self.origin_id = origin_id

    def __enter__(self):
        self.profile.mkdir(exist_ok=True)
        uuids = json.dumps({self.extension_id: self.origin_id})
        (self.profile / "user.js").write_text(
            f'user_pref("extensions.webextensions.uuids", {json.dumps(uuids)});\n'
            'user_pref("browser.shell.checkDefaultBrowser", false);\n'
            'user_pref("browser.aboutwelcome.enabled", false);\n'
        )
        self.socket = None
        with self.log.open("wb") as log:
            self.process = subprocess.Popen([
                str(FIREFOX / "MacOS/firefox"), "--no-remote", "--profile", str(self.profile),
                "--remote-debugging-port", "0", "about:blank",
            ], stdout=log, stderr=log, env={**os.environ, "MOZ_REMOTE_ALLOW_SYSTEM_ACCESS": "1"})
        try:
            def endpoint():
                require(self.process.poll() is None, "Firefox exited during startup")
                return re.search(r"WebDriver BiDi listening on (ws://[^\s]+)", self.log.read_text())

            url = wait("Firefox BiDi endpoint", endpoint, 30).group(1)
            self.socket = websocket.create_connection(url + "/session", timeout=15, suppress_origin=True)
            self.call("session.new", {"capabilities": {}})
            return self
        except BaseException:
            self.__exit__()
            raise

    def send(self, message):
        self.socket.send(message)

    def receive(self, timeout):
        self.socket.settimeout(timeout)
        return self.socket.recv()

    def __exit__(self, *_):
        if self.socket:
            try:
                self.call("browser.close")
            except (OSError, RuntimeError, websocket.WebSocketException):
                pass
            self.socket.close()
        else:
            self.process.terminate()
        try:
            self.process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
            raise RuntimeError("Firefox did not shut down cleanly")

    def install(self, package):
        result = self.call("webExtension.install", {
            "extensionData": {"type": "path", "path": str(package)},
        })
        require(result["extension"] == self.extension_id, f"Unexpected add-on ID: {result}")
        return "moz-extension://" + self.origin_id

    def open(self, url):
        context = self.call("browsingContext.create", {"type": "tab"})["context"]
        self.call("browsingContext.activate", {"context": context})
        self.call("browsingContext.navigate", {"context": context, "url": url, "wait": "none"})
        return context

    def evaluate(self, page, expression):
        result = self.call("script.evaluate", {
            "expression": f"JSON.stringify(({expression}))",
            "target": {"context": page}, "awaitPromise": False,
        })
        require(result["type"] == "success", f"Firefox script error: {result}")
        value = result["result"].get("value")
        return json.loads(value) if value is not None else None

    def screenshot(self, page):
        return self.call("browsingContext.captureScreenshot", {"context": page})["data"]


PROBE = """(() => {
  const events = [];
  const record = (type, detail = null) => events.push({
    type, detail, visibility: document.visibilityState, role: globalThis.graphshellViewRole,
    time: performance.timeOrigin + performance.now(),
  });
  globalThis.hiddenWriterEvidence = events;
  record("attached");
  for (const type of ["visibilitychange", "graphshell-edit-submit", "graphshell-edit-admit",
                      "graphshell-edit-admitted", "graphshell-edit-reply", "graphshell-committed"]) {
    document.addEventListener(type, event => record(type, event.detail ? JSON.parse(event.detail) : null));
  }
  return true;
})()"""

STATE = """(() => {
  const root = document.querySelector("graphshell-view, graphshell-tree");
  return {
    url: location.href, userAgent: navigator.userAgent, title: document.title,
    role: globalThis.graphshellViewRole, visibility: document.visibilityState,
    dataset: root ? {...root.dataset} : {},
    events: globalThis.hiddenWriterEvidence || [],
    errors: globalThis.graphshellGateFailures || [],
    consoleErrors: globalThis.graphshellErrors || [],
    semantic: globalThis.graphshellSemanticTree?.(),
  };
})()"""


def button(label):
    return f"""(() => {{
      const label = {json.dumps(label)};
      const button = [...document.querySelectorAll('button, [role="button"]')].find(node => {{
        const text = node.getAttribute("aria-label") || node.textContent.trim();
        return (text === label || text === label.repeat(2)) && node.getClientRects().length;
      }});
      if (!button || button.disabled) return false;
      button.click();
      return true;
    }})()"""


def field(label, value):
    return f"""(() => {{
      const field = document.querySelector('[role="textbox"][aria-label="{label}"]');
      if (!field) return false;
      if (field.textContent === {json.dumps(value)}) return true;
      field.focus();
      for (const key of {json.dumps(value)}) {{
        field.dispatchEvent(new KeyboardEvent("keydown", {{key, bubbles: true, cancelable: true}}));
      }}
      return true;
    }})()"""


def observe(browser, page, label, predicate, receipt, timeout=60):
    def check():
        state = browser.evaluate(page, STATE)
        receipt[label] = state
        require(not state["errors"], f"{label}: page errors {state['errors']}")
        require(not state["title"].startswith("GRAPHSHELL H3 FAIL"), state["title"])
        return state if predicate(state) else None

    return wait(label, check, timeout)


def ready(state, role, visibility):
    return (state.get("role") == role and state["visibility"] == visibility
            and state["dataset"].get("ready") == "true")


def labels(node, title):
    if not node:
        return 0
    return int(node.get("label") == title) + sum(labels(child, title) for child in node.get("children", []))


def events(state, kind):
    return [event for event in state["events"] if event["type"] == kind]


def scenario(browser, url, receipt, out):
    writer = browser.open(url)
    observe(browser, writer, "writer_initial", lambda s: ready(s, "writer", "visible"), receipt)
    browser.evaluate(writer, PROBE)
    follower = browser.open(url)
    observe(browser, follower, "follower_initial", lambda s: ready(s, "follower", "visible"), receipt)
    browser.evaluate(follower, PROBE)
    observe(browser, writer, "writer_before", lambda s: ready(s, "writer", "hidden"), receipt)
    title = receipt["title"]
    require(labels(receipt["follower_initial"]["semantic"], title) == 0, "Object already present")
    wait("Open Panes", lambda: browser.evaluate(follower, button("Panes")), timeout=12)
    wait("Open Document tools", lambda: browser.evaluate(follower, button("Document tools")), timeout=12)
    observe(browser, follower, "document_tools_open",
            lambda s: labels(s["semantic"], "Add an object"), receipt, timeout=12)
    wait("Open intake", lambda: browser.evaluate(follower, button("Add an object")), timeout=12)
    wait("Enter address", lambda: browser.evaluate(follower, field("Address", receipt["address"])), timeout=12)
    wait("Enter title", lambda: browser.evaluate(follower, field("New object title", title)), timeout=12)
    for label, value in [("Address", receipt["address"]), ("New object title", title)]:
        selector = json.dumps(f'[role="textbox"][aria-label="{label}"]')
        wait(f"Confirm {label}", lambda: browser.evaluate(
            follower, f"document.querySelector({selector})?.textContent === {json.dumps(value)}"), timeout=12)
    receipt["follower_before_submit"] = browser.evaluate(follower, STATE)
    wait("Submit from follower", lambda: browser.evaluate(follower, button("New node")), timeout=12)
    # Once submitted, leave the hidden writer alone until the follower has an
    # answer. These DOM listeners record events without driving either loop.
    acknowledged = observe(browser, follower, "follower_ack",
                           lambda s: events(s, "graphshell-edit-reply"), receipt, timeout=40)
    submissions = events(acknowledged, "graphshell-edit-submit")
    replies = events(acknowledged, "graphshell-edit-reply")
    require(len(submissions) == len(replies) == 1, "Expected one submission and one reply")
    require(submissions[0]["detail"]["edit"] == {
        "kind": "add_node", "address": receipt["address"], "title": title,
    }, "Wrong submitted edit")
    reply = replies[0]["detail"]
    require(reply["id"] == submissions[0]["detail"]["id"] and reply["status"] == "admitted",
            f"Follower edit was not admitted: {reply}")
    refreshed = observe(browser, follower, "follower_reread",
                        lambda s: ready(s, "follower", "visible") and labels(s["semantic"], title), receipt)
    require(labels(refreshed["semantic"], title) == 1, "Follower rendered duplicate objects")
    written = observe(browser, writer, "writer_after", lambda s: ready(s, "writer", "hidden"), receipt)
    writer_admit = events(written, "graphshell-edit-admit")
    writer_ack = events(written, "graphshell-edit-admitted")
    require(len(writer_admit) == len(writer_ack) == 1, "Expected one writer admission and one writer ack")
    require(writer_ack[0]["detail"] == reply, "Writer and follower acknowledgement mismatch")
    require(all(e["visibility"] == "hidden" and e["role"] == "writer" for e in writer_admit + writer_ack),
            "Writer was visible or changed roles during admission")
    hidden = events(written, "visibilitychange")
    require(len(hidden) == 1 and hidden[0]["visibility"] == "hidden"
            and hidden[0]["time"] <= submissions[0]["time"], "Writer visibility changed during the scenario")
    require(all(e["visibility"] == "visible" and e["role"] == "follower" for e in submissions + replies),
            "Follower was hidden or changed roles")
    (out / "follower-reread.png").write_bytes(base64.b64decode(browser.screenshot(follower)))


def sha256(path):
    with path.open("rb") as stream:
        digest = hashlib.sha256()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
        return digest.hexdigest()


def qualify(name, package, out, marker):
    out.mkdir()
    app = CHROME if name == "chrome" else FIREFOX
    token = uuid.uuid4().hex[:10]
    receipt = {
        "browser": name, "browser_app": str(app.parent), "headed": True,
        "platform": platform.platform(), "scenario": "hidden-writer-follower-add-node",
        "producer_marker": marker,
        "started_utc": datetime.now(timezone.utc).isoformat(), "package": str(package),
        "title": f"Hidden writer {token}", "address": f"https://qualification.invalid/{token}",
        "result": "fail",
    }
    try:
        with (app / "Info.plist").open("rb") as stream:
            receipt["browser_version"] = plistlib.load(stream)["CFBundleShortVersionString"]
        manifest = json.loads((package / "manifest.json").read_text())
        receipt["extension_version"] = manifest["version"]
        receipt["sha256"] = {
            str(path.relative_to(package)): sha256(path)
            for path in sorted(package.rglob("*")) if path.is_file()
        }
        with tempfile.TemporaryDirectory(prefix=f"mere-{name}-") as temp:
            profile = Path(temp) / "profile"
            origin_id = str(uuid.uuid4())

            def launch(phase):
                log = out / f"{phase}.log"
                if name == "chrome":
                    return Chrome(profile, log)
                extension_id = manifest["browser_specific_settings"]["gecko"]["id"]
                return Firefox(profile, log, extension_id, origin_id)

            with launch("initial") as browser:
                url = browser.install(package) + "/graph.html"
                scenario(browser, url, receipt, out)
            with launch("reopen") as browser:
                reopened_url = browser.install(package) + "/graph.html"
                require(reopened_url == url, "Extension origin changed at reopen")
                page = browser.open(reopened_url)
                reopened = observe(browser, page, "fresh_process_reopen",
                                   lambda s: ready(s, "writer", "visible")
                                   and labels(s["semantic"], receipt["title"]), receipt)
                require(labels(reopened["semantic"], receipt["title"]) == 1, "Reopen rendered duplicate objects")
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
    parser.add_argument("--marker-timeout", type=int, default=1800)
    parser.add_argument("--chrome-package", type=Path, default=Path("/tmp/mere-extension-chromium"))
    parser.add_argument("--firefox-package", type=Path, default=Path("/tmp/mere-extension-firefox"))
    args = parser.parse_args()
    wait(f"Waiting for {args.marker}", args.marker.is_file, args.marker_timeout)
    marker = args.marker.read_text().strip()
    args.out.mkdir(parents=True)
    browsers = ["chrome", "firefox"] if args.browser == "both" else [args.browser]
    results = [qualify(name, getattr(args, f"{name}_package").resolve(), args.out / name, marker)
               for name in browsers]
    return 0 if all(results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
