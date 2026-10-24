#!/usr/bin/env python3
"""Select native desktop and optional forced-relay network providers."""
import os
from pathlib import Path
import native_machine as base

if os.environ.get('GRAPHSHELL_RELAY_ACCEPTANCE_URL'):
    import network_machine  # Installs the forced-relay Machine on the shared base.

Original = base.Machine


class Machine(Original):
    def info(self):
        value = super().info()
        value['worker_sha256'] = base.sha(Path(__file__))
        return value


base.Machine = Machine
if __name__ == '__main__':
    base.main()
