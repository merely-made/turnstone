# Capture golden fixtures

Copied unchanged from Eidetic `crates/eidetic-core/tests/fixtures/capture_golden/`
at rev `98f2c69` (memory-gates plan S6a). They are bytes Turnstone's own
capture core wrote at `cc9cc6d2`, on synthetic input and with the XOR test
sealer, before that core was promoted into `eidetic::capture`.

Eidetic holds them to its stand-in annotator. `src/trail_memory/capture_golden_tests.rs`
holds Turnstone's real Fleece annotator to the same bytes, so the
annotation record, which Eidetic cannot build, is byte-identical too.

Only the families that carry a Fleece annotation are copied (sealed capture
and the unsealed source document). Do not regenerate to make a test pass: a
byte difference means the stored format moved, which is a migration.
