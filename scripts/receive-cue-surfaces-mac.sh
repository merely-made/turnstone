#!/usr/bin/env bash
# Run the finite same-host receiver in the logged-in macOS GUI session.
# Retain the binary identity, all output and failed runs. A live process or a
# successful LaunchServices call is not a passing receiving result.
set -euo pipefail
BINARY="${1:?supply the already-built cue_surfaces executable}"
RUN_ROOT="${2:?supply a fresh absolute receipt directory}"
[[ -x "$BINARY" ]] || { echo "Executable missing: $BINARY" >&2; exit 1; }
[[ "$RUN_ROOT" = /* && ! -e "$RUN_ROOT" ]] || { echo 'Receipt directory must be fresh and absolute' >&2; exit 1; }
MODE="${3:-headed}"
EXTRA_ARGS=()
case "$MODE" in headed) ;; offscreen) EXTRA_ARGS+=(--offscreen) ;; *) echo "Mode must be headed or offscreen" >&2; exit 1 ;; esac
APP="$RUN_ROOT/CueReceiver.app"
EXE="$APP/Contents/MacOS/cue_surfaces"
mkdir -p "$APP/Contents/MacOS"
cp "$BINARY" "$EXE"
PLIST="$APP/Contents/Info.plist"
plutil -create xml1 "$PLIST"
plutil -insert CFBundleExecutable -string cue_surfaces "$PLIST"
plutil -insert CFBundleIdentifier -string made.merely.turnstone.cue-receiver "$PLIST"
plutil -insert CFBundleName -string CueReceiver "$PLIST"
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
    --env WGPU_BACKEND=metal "$APP" --args "$RUN_ROOT/receiving" "${EXTRA_ARGS[@]}"
deadline=$((SECONDS + 120))
launch_deadline=$((SECONDS + 10))
saw_process=0
while [[ $SECONDS -lt $deadline ]]; do
    if app_running; then saw_process=1; elif [[ $saw_process -eq 1 || $SECONDS -ge $launch_deadline ]]; then break; fi
    sleep 0.2
done
if app_running; then echo 'Receiver exceeded bounded lifetime' >&2; exit 1; fi
python3 - "$RUN_ROOT/receiving/qualification.json" "$MODE" <<'PY'
import json,sys
from pathlib import Path
p=Path(sys.argv[1])
assert p.exists(), 'receiver exited without qualification; inspect stderr.log'
q=json.loads(p.read_text());assert q.get('passed') is True,q
assert q['qualification_kind']==sys.argv[2]+'-native-receiving',q
if sys.argv[2]=='headed': assert q['window_presentations']>0,q
print('Real Metal Cue receiving passed:',p)
PY
