"""Boundary and discovery controls for the source-size gate."""

from pathlib import Path
import tempfile
import unittest

from check_source_size import violations


class SourceSizeTests(unittest.TestCase):
    def test_exact_limit_passes_but_an_untracked_601st_line_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "src/new.rs"
            path.parent.mkdir()
            path.write_text("// line\n" * 600)
            self.assertEqual(violations(root), [])
            path.write_text("// line\n" * 601)
            self.assertEqual(violations(root), [(Path("src/new.rs"), 601)])

    def test_tests_and_examples_are_checked_but_vendored_sources_are_not_ours(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["tests/control.rs", "examples/control.rs", "build.rs", "sandbox_bootstrap_win/src/lib.rs", "vendor/supplier.rs"]:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("// line\n" * 601)
            self.assertEqual({str(p) for p, _ in violations(root)},
                             {"tests/control.rs", "examples/control.rs", "build.rs", "sandbox_bootstrap_win/src/lib.rs"})


if __name__ == "__main__":
    unittest.main()
