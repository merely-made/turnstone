"""Preserve the Git candidate and prepare a bounded local supplier control."""
import datetime
import hashlib
import json
import pathlib
import tomllib

receipt = pathlib.Path(__file__).resolve().parent
root = receipt.parents[2]
control = receipt / "sync-diagnostic"
config = root / ".cargo/config.toml"
assert not config.exists(), "Preserve an existing Cargo config; do not replace it"
control.mkdir(exist_ok=False)
original = (root / "Cargo.lock").read_bytes()
manifest = (root / "Cargo.toml").read_bytes()
(control / "Cargo.lock.git-candidate").write_bytes(original)
(control / "Cargo.toml.git-candidate").write_bytes(manifest)
source = "git+https://github.com/merely-made/wgpu-graft.git?rev=dec11bbd2c9c8676e66987fb6ca32cd4ae6310eb#dec11bbd2c9c8676e66987fb6ca32cd4ae6310eb"
old = tomllib.loads(original.decode())
line = f'source = "{source}"'
assert original.count(line.encode()) == 3
modified = original.replace((line + "\n").encode(), b"")
assert modified != original
suffix = f" ({source.split('#')[0]})".encode()
reference_count = modified.count(suffix)
modified = modified.replace(suffix, b"")
new = tomllib.loads(modified.decode())
names = {"grafting", "grafting-frame", "servo-wgpu-interop-adapter"}
expected = []
changes = []
for package in old["package"]:
    item = dict(package)
    if item.get("source") == source:
        assert item["name"] in names
        changes.append({"name": item["name"], "version": item["version"], "source": "local diagnostic path"})
        del item["source"]
    if "dependencies" in item:
        item["dependencies"] = [value.replace(suffix.decode(), "") for value in item["dependencies"]]
    expected.append(item)
assert len(changes) == 3 and new["package"] == expected
assert {key: value for key, value in old.items() if key != "package"} == {key: value for key, value in new.items() if key != "package"}
patch = '[patch."https://github.com/merely-made/wgpu-graft.git"]\n' + "".join(
    f'{name} = {{ path = "C:/Users/mark_/Code/repos/wgpu-graft/{name}" }}\n'
    for name in sorted(names)
)
config.write_text(patch, encoding="utf-8", newline="\n")
(root / "Cargo.lock").write_bytes(modified)
record = {
    "recorded_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "scope": "Temporary local diagnostic, not a publication or final U9 repin",
    "original_config": "absent; existing config.local.toml and local directory untouched",
    "config_sha256": hashlib.sha256(config.read_bytes()).hexdigest(),
    "manifest_sha256": hashlib.sha256(manifest).hexdigest(),
    "original_lock_sha256": hashlib.sha256(original).hexdigest(),
    "control_lock_sha256": hashlib.sha256(modified).hexdigest(),
    "packages": len(expected),
    "only_changed_package_sources": changes,
    "rewritten_references": reference_count,
    "other_package_records_unchanged": True,
}
(control / "prepare.json").write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
print(json.dumps(record, indent=2))
