#!/usr/bin/env python3
"""One active entry point over the frozen native machine and network providers."""
import importlib.util
import os
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = ROOT / 'ports/graphshell/docs/receipts'
sys.path.insert(0, str(RECEIPTS / ('coop_relay_20261020' if os.environ.get('GRAPHSHELL_RELAY_ACCEPTANCE_URL') else 'coop_mac_thinkpad_20261020')))
spec = importlib.util.spec_from_file_location('native_machine_provider', Path(sys.path[0]) / 'machine.py')
provider = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provider)
if hasattr(provider, 'base'):
    base = provider.base
else:
    base = provider
Original = base.Machine
class Machine(Original):
    def info(self):
        value = super().info()
        value['worker_sha256'] = base.sha(Path(__file__))
        return value
base.Machine = Machine
if __name__ == '__main__':
    base.main()
