"""Collect completed integration checks and hash the immutable evidence."""
import datetime
import hashlib
import json
import pathlib
import re
import subprocess

r = pathlib.Path(__file__).resolve().parent
subprocess.run(["python", str(r / "fingerprint.py"), "--verify"], check=True)
source = json.loads((r / "source-manifest.json").read_text())
prior = json.loads((r.parent / "browser_scry_windows_20261006/final-source-manifest.json").read_text())
old = {v["path"]: v["sha256"] for v in prior["files"]}
changed = [v["path"] for v in source["files"] if old[v["path"]] != v["sha256"]]
assert set(changed) == {"Cargo.toml", "Cargo.lock"}, changed
def read(path):
    data = path.read_bytes()
    return data.decode("utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig")
tests = {}
for name in ("shell", "find", "chrome"):
    log = read(r / f"{name}-tests.log")
    match = re.search(r"test result: ok\. (\d+) passed; 0 failed;", log)
    if not match or int(match[1]) == 0:
        raise SystemExit(f"Missing passing {name} result")
    tests[name] = int(match[1])
runs = {}
for name in ("native-weld-final-input", "native-scry-final",
             "native-scry-final-restart", "native-weld-permission-final"):
    result = read(r / name / "scenario.done").splitlines()[0]
    if result != "RESULT ok":
        raise SystemExit(f"Failed integration native run: {name}")
    runs[name] = dict(result=result, native_exit=0,
                     evidence="Runner completed after explicit exit/result checks")
assert read(r / "permission-server.receipt").startswith("RESULT ok")
closed = json.loads((r / "native-scry-final/closed.surface-frames.json").read_text())
assert closed["live_producers"] == closed["cached_frames"] == 0
assert not closed["scry_importers"]
reviews = {
    "native-weld-final-input/01b_independent_weld_input.png": "Visible alpha/bravo, independent clicks and cookies",
    "native-weld-final-input/02_find.png": "One highlighted A match and chrome targets A; B remains separate",
    "native-weld-final-input/03_zoom.png": "Only A visibly scales, about 1.1 versus B 1; requested zoom remains Partial",
    "native-weld-final-input/04_teardown.png": "Both content surfaces off without residual page imagery",
    "native-scry-final/02_text_pointer_and_keyboard.png": "Current independent input/clicks and retained site state",
    "native-scry-final/06_a_scry_reconstructed.png": "Both reconstructed/sibling pages visibly populated",
    "native-scry-final/07_both_closed.png": "Both pages off; structural snapshot separately confirms zero state",
    "native-scry-final/08_ready_for_restart.png": "Both reopened pages visibly populated",
    "native-scry-final-restart/01_restored_two_pages.png": "Separate process restores both populated pages and text",
    "native-weld-permission-final/01_permission_decision.png": "Real exact-origin location request and native decision controls",
    "native-weld-permission-final/02_permission_answered.png": "Denial callback navigates and renders result page",
}
for path in reviews:
    assert (r / path).is_file()
def save(name, record):
    (r / name).write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
save("facts.json", dict(recorded_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    source_manifest="source-manifest.json", exe_sha256=source["exe_sha256"],
    features=source["features"], changed_qualification_inputs=changed,
    browser_tests=tests, runs=runs, permission_server="RESULT ok", scry_closed=closed,
    full_library_suite_rerun=False, prior_broad_suite_clean=False,
    prior_suite_note="Prior snapshot's network sync-round test failed parallel and passed isolated; preserved in frozen prior receipt",
    limits=["Windows primary Workbench only", "selfdrive input, not physical keyboard/IME", "requested Weld zoom remains Partial", "denial callback, not successful geolocation", "DPR differs by engine", "Servo and next registry release remain open"],
    retained_state=dict(target="C:/t/cargo-targets/turnstone", target_owner="Existing shared Turnstone target",
                        profiles="Ignored synthetic exact-profile restart evidence", isolated_cargo_home=None, worktree=None)))
save("visual-review.json", dict(method="Direct inspection of native composed-frame PNGs", captures=reviews))
files = [p for p in sorted(r.rglob("*")) if p.is_file()
         and "profile" not in p.relative_to(r).parts
         and p.name != "artifact-manifest.json"]
save("artifact-manifest.json", dict(files=[dict(path=p.relative_to(r).as_posix(),
    bytes=p.stat().st_size, sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in files]))
print(f"Browser tests: {tests}; four native runs passed; {len(files)} evidence files hashed")
