"""Offline regression checks for nested lock selection and portable provenance."""
import os
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("cargo_mode.py")


class CargoModeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cargo-mode-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "repo"
        self.root.mkdir()
        self.env = dict(os.environ, CARGO_HOME=str(Path(self.temp.name) / "cargo-home"))
        self.env.pop("CARGO_RESOLVER_LOCKFILE_PATH", None)
        self.env["CARGO_TARGET_DIR"] = str(Path(self.temp.name) / "target")
        self.command(["git", "init", "-q"])
        (self.root / "scripts").mkdir()
        shutil.copy2(SCRIPT, self.root / "scripts/cargo_mode.py")
        for relative, name in [(".", "root_probe"), ("nested", "nested_probe")]:
            root = self.root / relative
            (root / "src").mkdir(parents=True)
            (root / "src/lib.rs").write_text("pub fn probe() {}\n")
            (root / "Cargo.toml").write_text(
                f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n'
                '[workspace]\nexclude = ["nested"]\n')
            self.command(["cargo", "generate-lockfile", "--offline"], cwd=root)
        (self.root / ".cargo").mkdir()
        (self.root / ".cargo/config.toml").write_text('[alias]\nprobe = "check"\n')
        self.command(["git", "add", "Cargo.toml", "Cargo.lock", "src", "nested"])

    def command(self, args, cwd=None, success=True):
        result = subprocess.run(args, cwd=cwd or self.root, env=self.env,
                                text=True, capture_output=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def mode(self, *args, **kwargs):
        import sys
        return self.command([sys.executable, str(self.root / "scripts/cargo_mode.py"), *args], **kwargs)

    def test_nested_manifest_and_cwd_preserve_both_locks(self):
        before = [(self.root / p).read_bytes() for p in ["Cargo.lock", "nested/Cargo.lock"]]
        self.mode("setup")
        self.mode("local", "generate-lockfile", "--offline")
        root_local = (self.root / ".cargo/local/Cargo.lock").read_bytes()
        self.mode("local", "generate-lockfile", "--offline", "--manifest-path", "nested/Cargo.toml")
        self.mode("local", "check", "--offline", "--locked", cwd=self.root / "nested")
        self.assertEqual((self.root / ".cargo/local/Cargo.lock").read_bytes(), root_local)
        self.assertIn(b'nested_probe', (self.root / "nested/.cargo/local/Cargo.lock").read_bytes())
        self.assertEqual(before, [(self.root / p).read_bytes() for p in ["Cargo.lock", "nested/Cargo.lock"]])
        self.mode("setup")  # Idempotent: preserve existing local resolution.
        self.assertEqual((self.root / ".cargo/local/Cargo.lock").read_bytes(), root_local)
        self.mode("verify", "--metadata-only")

    def test_portable_rejects_redirect_before_resolution(self):
        self.mode("setup")
        self.env["CARGO_RESOLVER_LOCKFILE_PATH"] = str(self.root / "elsewhere/Cargo.lock")
        result = self.mode("verify", "--metadata-only", success=False)
        self.assertIn("environment override", result.stderr)
        self.env.pop("CARGO_RESOLVER_LOCKFILE_PATH")
        (self.root / ".cargo/config.toml").write_text('[resolver]\nlockfile-path = "elsewhere/Cargo.lock"\n')
        result = self.mode("verify", "--metadata-only", success=False)
        self.assertIn("redirect-free", result.stderr)

    def test_portable_rejects_external_path_package(self):
        self.mode("setup")
        external = self.root.parent / "external"
        (external / "src").mkdir(parents=True)
        (external / "src/lib.rs").write_text("")
        (external / "Cargo.toml").write_text('[package]\nname="external"\nversion="0.1.0"\n')
        with (self.root / "Cargo.toml").open("a") as manifest:
            manifest.write('[dependencies]\nexternal = { path = "../external" }\n')
        self.command(["cargo", "generate-lockfile", "--offline"])
        result = self.mode("verify", "--metadata-only", success=False)
        self.assertIn("Outside path package", result.stderr)

    def test_local_config_keeps_relative_patch_base(self):
        self.mode("setup")
        external = self.root.parent / "external"
        (external / "src").mkdir(parents=True)
        (external / "src/lib.rs").write_text("")
        (external / "Cargo.toml").write_text('[package]\nname="cargo-mode-fixture-unique"\nversion="0.1.0"\n')
        with (self.root / "Cargo.toml").open("a") as manifest:
            manifest.write('[dependencies]\ncargo-mode-fixture-unique = "0.1.0"\n')
        with (self.root / ".cargo/config.local.toml").open("a") as config:
            config.write('[patch.crates-io]\ncargo-mode-fixture-unique = { path = "../external" }\n')
        before = (self.root / "Cargo.lock").read_bytes()
        result = self.mode("local", "metadata", "--offline", "--format-version", "1")
        packages = json.loads(result.stdout)["packages"]
        selected = next(p for p in packages if p["name"] == "cargo-mode-fixture-unique")
        self.assertEqual(Path(selected["manifest_path"]).resolve(), external / "Cargo.toml")
        self.assertEqual((self.root / "Cargo.lock").read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
