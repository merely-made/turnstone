#!/usr/bin/env python3
"""Freeze real native artifacts and every executed acceptance provider."""
import importlib.util
import json
from pathlib import Path
import sys
root = Path(sys.argv[sys.argv.index('--repo')+1]).resolve()
out = Path(sys.argv[sys.argv.index('--output')+1]).resolve()
receipts = root / 'ports/graphshell/docs/receipts'
spec = importlib.util.spec_from_file_location('native_freeze', receipts / 'coop_mac_thinkpad_20261020/freeze.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
base.main()
p = out / 'build-inputs.json'
value = json.loads(p.read_text())
files = [Path('support/graphshell-coop')/n for n in ['run.py','machine.py','freeze.py','relay.py','verify.py']]
files += [Path('ports/graphshell/docs/receipts')/n for n in ['coop_mac_thinkpad_20261020/machine.py','coop_mac_thinkpad_20261020/freeze.py','coop_relay_20261020/machine.py','coop_20261020/resident.py','coop_relay_20261020/relay/Cargo.toml','coop_relay_20261020/relay/Cargo.lock','coop_relay_20261020/relay/src/main.rs']]
value['harness'] = {str(n):base.sha(root/n) for n in files}
p.write_text(json.dumps(value,indent=2)+'\n')
