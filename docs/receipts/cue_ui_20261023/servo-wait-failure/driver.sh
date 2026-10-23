#!/usr/bin/env bash
# Exercise the real Turnstone UI in a fresh synthetic profile. The .app and
# stdout remain local; captures, exact scenario and outcome form the receipt.
set -euo pipefail
BINARY="${1:?supply an already-built Turnstone executable}"
SCENARIO="${2:?supply an absolute scenario path}"
RUN_ROOT="${3:?supply a fresh absolute receipt directory}"
FIXTURE_BASE="${4:?supply the synthetic loopback fixture base URL}"
BUDGET="${5:-480}"
[[ "$BUDGET" =~ ^[1-9][0-9]*$ ]] || exit 1
[[ -x "$BINARY" && -f "$SCENARIO" ]] || exit 1
[[ "$RUN_ROOT" = /* && ! -e "$RUN_ROOT" ]] || exit 1
APP="$RUN_ROOT/TurnstoneCue.app"
EXE="$APP/Contents/MacOS/turnstone"
mkdir -p "$APP/Contents/MacOS" "$RUN_ROOT/receiving"
cp "$BINARY" "$EXE"
cp "$SCENARIO" "$RUN_ROOT/scenario.scn"
cp "$0" "$RUN_ROOT/driver.sh"
PLIST="$APP/Contents/Info.plist"
plutil -create xml1 "$PLIST"
plutil -insert CFBundleExecutable -string turnstone "$PLIST"
plutil -insert CFBundleIdentifier -string made.merely.turnstone.cue-ui-receiver "$PLIST"
plutil -insert CFBundleName -string TurnstoneCue "$PLIST"
plutil -insert CFBundlePackageType -string APPL "$PLIST"
plutil -insert NSHighResolutionCapable -bool true "$PLIST"
shasum -a 256 "$EXE" > "$RUN_ROOT/binary.sha256"
app_running() {
    ps -ax -o command= | awk -v exe="$EXE" 'index($0, exe) == 1 { found=1 } END { exit !found }'
}
stop_own_app() {
    while read -r pid command; do
        if [[ "$command" = "$EXE"* ]]; then kill "$pid" 2>/dev/null || true; fi
    done < <(ps -ax -o pid= -o command=)
}
trap stop_own_app EXIT
open -n -F --stdout "$RUN_ROOT/stdout.log" --stderr "$RUN_ROOT/stderr.log" \
    --env WGPU_BACKEND=metal --env RUST_LOG=turnstone=info,turnstone::shell::input=debug --env TURNSTONE_ROOT="$RUN_ROOT/profile" \
    --env MERE_ROOT="$RUN_ROOT/mere" --env TURNSTONE_SCENARIO="$RUN_ROOT/scenario.scn" \
    --env TURNSTONE_CAPTURE_DIR="$RUN_ROOT/receiving" \
    --env TURNSTONE_SERVO_PROFILE=CueUi --env SERVO_FIXTURE_BASE="$FIXTURE_BASE" "$APP"
deadline=$((SECONDS + BUDGET))
launch_deadline=$((SECONDS + 10))
saw_process=0
while [[ $SECONDS -lt $deadline ]]; do
    if app_running; then saw_process=1; elif [[ $saw_process -eq 1 || $SECONDS -ge $launch_deadline ]]; then break; fi
    sleep 0.2
done
if app_running; then echo 'Turnstone exceeded bounded lifetime' >&2; exit 1; fi
python3 - "$RUN_ROOT/receiving/scenario.done" "$RUN_ROOT/receiving" <<'PY'
import json,sys
from pathlib import Path
outcome=Path(sys.argv[1]); assert outcome.exists(),'Turnstone exited without an outcome'
assert outcome.read_text().splitlines()[0]=='RESULT ok',outcome.read_text()
frames=list(Path(sys.argv[2]).glob('*.cue-frames.json'))
assert frames,'no shared-presentation evidence'
entries=[e for p in frames for e in json.loads(p.read_text())['entries']]
assert any(e['sampled_layers']>0 and e['scope'] for e in entries),'no sampled shared frame'
for lens in Path(sys.argv[2]).glob('*lens.cue-frames.json'):
    assert any(e['sampled_layers']>0 and e['scope'] for e in json.loads(lens.read_text())['entries']), f'lens has no sampled shared content: {lens}'
assert list(Path(sys.argv[2]).glob('*.png')),'no composed capture'
print('Turnstone UI Cue receipt passed:',outcome)
PY
