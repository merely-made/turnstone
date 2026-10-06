"""Verify the unapplied consumer patch without changing production source."""
from pathlib import Path
import hashlib
import json
import re
import subprocess


def candidate(base, patch):
    lines = base.splitlines(True)
    result = []
    position = 0
    active = False
    for line in patch.splitlines(True):
        if line.startswith("@@"):
            start = int(re.match(r"@@ -(\d+)", line)[1]) - 1
            result.extend(lines[position:start])
            position = start
            active = True
        elif active and line.startswith((" ", "-", "+")):
            if line[0] != "+":
                assert lines[position] == line[1:], (position, line)
                position += 1
            if line[0] != "-":
                result.append(line[1:])
    result.extend(lines[position:])
    return "".join(result)


if __name__ == "__main__":
    receipt = Path(__file__).resolve().parent
    root = receipt.parents[2]
    preparation = json.loads((receipt / "preparation.json").read_text())
    source = root / "src/behaviors.rs"
    patch_path = receipt / "behaviors.patch"
    assert hashlib.sha256(source.read_bytes()).hexdigest() == preparation["consumer_base_sha256"]
    assert hashlib.sha256(patch_path.read_bytes()).hexdigest() == preparation["patch_sha256"]
    proposed = candidate(source.read_text(), patch_path.read_text())
    assert hashlib.sha256(proposed.encode()).hexdigest() == preparation["candidate_sha256_lf"]
    subprocess.run(["git", "apply", "--check", str(patch_path)], cwd=root, check=True)
    subprocess.run(
        ["rustfmt", "--edition", "2024", "--emit", "stdout"],
        input=proposed.encode(), stdout=subprocess.DEVNULL, check=True,
    )
    print("Verified hashes, patch applicability and Rust parsing; no compilation or regression claim.")
