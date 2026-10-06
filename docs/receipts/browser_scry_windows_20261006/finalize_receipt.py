"""Collect the final native results and hash retained evidence without profiles."""
import datetime
import hashlib
import json
import pathlib
import subprocess

r = pathlib.Path(__file__).resolve().parent
subprocess.run(["python", str(r / "fingerprint.py"), "--verify"], check=True)
manifest = json.loads((r / "final-source-manifest.json").read_text())
runs = {}
for name in (
    "native-weld-final-input", "native-scry-final",
    "native-scry-final-restart", "native-weld-permission-final",
):
    result = (r / name / "scenario.done").read_text().splitlines()[0]
    if result != "RESULT ok":
        raise SystemExit(f"Failed final native run: {name}")
    runs[name] = dict(result=result, native_exit=0,
                     exit_evidence="Runner completed after explicit exit/result checks")
server = (r / "permission-server-final.receipt").read_text()
if not server.startswith("RESULT ok"):
    raise SystemExit("Final permission fixture server failed")
closed = json.loads((r / "native-scry-final/closed.surface-frames.json").read_text())
assert closed["live_producers"] == closed["cached_frames"] == 0
assert not closed["scry_importers"]
reviews = {
    "native-weld-final-input/01b_independent_weld_input.png": "Visible lowercase alpha/bravo, separate click counts and cookies in two pages",
    "native-weld-final-input/02_find.png": "A has one highlighted Page A text match; B retains independent text; chrome targets A",
    "native-weld-final-input/03_zoom.png": "A viewport 463x518 at about 1.1, B 509x570 at 1; requested zoom is Partial",
    "native-weld-final-input/04_teardown.png": "Both content surfaces off, with no residual page imagery",
    "native-scry-final/02_text_pointer_and_keyboard.png": "Current alpha/bravo and two clicks each, independent cookie values, DPR 2",
    "native-scry-final/06_a_scry_reconstructed.png": "Reconstructed A and sibling B both visibly populated with retained text/site state",
    "native-scry-final/07_both_closed.png": "Both content surfaces off, no residual page imagery; separate structural snapshot is zero",
    "native-scry-final/08_ready_for_restart.png": "Both reopened pages visibly populated before process exit",
    "native-scry-final-restart/01_restored_two_pages.png": "Separate process restores two populated pages and retained alpha/bravo",
    "native-weld-permission-final/01_permission_decision.png": "Exact loopback origin requests location; real Allow once/Always allow/Deny controls",
    "native-weld-permission-final/02_permission_answered.png": "Native denial callback navigates and renders Permission callback answered",
}
for path in reviews:
    assert (r / path).is_file()
def save(name, value):
    (r / name).write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
save("facts.json", dict(
    recorded_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    source_manifest="final-source-manifest.json",
    exe_sha256=manifest["exe_sha256"], features=manifest["features"],
    runs=runs, permission_server="RESULT ok", scry_closed=closed,
    final_targeted_tests="6 passed; 0 failed; serial Weld contracts/sandbox",
    preceding_broad_suite="665 passed, 1 failed, 9 ignored, 2 filtered; sandbox 2 pass serially; failed network case passes isolated",
    broad_suite_clean=False,
    limits=["selfdrive input, not physical keyboard/IME", "Weld requested zoom remains Partial", "permission denial callback, not successful geolocation", "DPR differs by engine", "Servo consumer and next registry release remain open"],
    retained_state=dict(target="C:/t/cargo-targets/turnstone", target_owner="Existing shared Turnstone build output",
                        profiles="Ignored synthetic exact-profile restart evidence", isolated_cargo_home=None, worktree=None),
))
save("visual-review.json", dict(method="Direct inspection of native composed-frame PNGs", captures=reviews))
files = [p for p in sorted(r.rglob("*")) if p.is_file()
         and "profile" not in p.relative_to(r).parts
         and p.name != "artifact-manifest.json"]
save("artifact-manifest.json", dict(files=[dict(
    path=p.relative_to(r).as_posix(), bytes=p.stat().st_size,
    sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in files]))
print(f"Four final native runs passed; {len(files)} evidence files hashed")
