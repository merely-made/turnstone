# Copyright 2026 Mark Alan Boykin
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# SPDX-License-Identifier: MPL-2.0

"""Compare changed documentation against a same-checkout published baseline."""
from pathlib import Path
from dataclasses import asdict
from unittest.mock import patch
import importlib.util
import json
import subprocess
import sys

from common import parser, fresh_output, check_budget
cli = parser(__doc__)
cli.add_argument("--baseline", default="origin/main")
args = cli.parse_args()
repo = args.repo.resolve()
out = fresh_output(args.output, repo)
sys.path.insert(0, str(repo / "scripts"))
spec = importlib.util.spec_from_file_location("instrument_doc_audit", repo / "scripts/mere_doc_audit.py")
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
baseline_ref = args.baseline
changed = subprocess.check_output(["git", "diff", "--name-only", baseline_ref, "--", "design_docs"], cwd=repo, text=True).splitlines()
for name in changed:
    if not (repo / name).is_file() or subprocess.run(["git", "cat-file", "-e", f"{baseline_ref}:{name}"], cwd=repo, capture_output=True).returncode:
        raise SystemExit("Added/deleted docs need a full-tree audit comparison; no read-text-only baseline claim: " + name)
prior = {repo / name: subprocess.check_output(["git", "show", f"{baseline_ref}:{name}"], cwd=repo, text=True, encoding="utf-8") for name in changed}
read = module.read_text
with patch.object(module, "read_text", lambda path: prior.get(path, read(path))):
    baseline = asdict(module.audit(repo))
current = asdict(module.audit(repo))
added = {}
for key, values in current.items():
    if isinstance(values, list):
        old = {json.dumps(v, sort_keys=True) for v in baseline[key]}
        new = [v for v in values if json.dumps(v, sort_keys=True) not in old]
        if new:
            added[key] = new
result = {"baseline_head": subprocess.check_output(["git", "rev-parse", baseline_ref], cwd=repo, text=True).strip(), "changed_docs": changed, "new_findings": added, "baseline_counts": {k: len(v) for k,v in baseline.items() if isinstance(v,list)}, "current_counts": {k: len(v) for k,v in current.items() if isinstance(v,list)}}
(out / "doc-audit.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({"changed_docs": changed, "new_findings": added}))
check_budget(out, args.max_bytes)
raise SystemExit(bool(added))
