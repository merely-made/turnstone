# Servo ordering diagnostic, October 7

This is a temporary local-source comparison, not the final U9 dependency set
or a production synchronization change. The normal constructor still selects
`Existing`. Explicit diagnostic modes select producer completion, exact
normalization-submission completion with a configured timeout, or both. There
is no CPU pixel-readback fallback. GL completion itself has no timeout; every
native process has the separate 180-second process guard.

The [original Git candidate lock](Cargo.lock.git-candidate) and
[manifest](Cargo.toml.git-candidate) are preserved before the ignored local
Cargo config is created. Resolution changes exactly the three Graft package
sources and their two dependency references. All 1,673 package records
otherwise remain identical, allowing record ordering to change. The initial
manual reference rewrite was incomplete and locked metadata refused it;
[offline resolution](offline-resolution-diff.json) corrected only those two
references before any build. The earlier failed metadata log is preserved.

The [supplier source manifest](supplier-source-manifest.json) and verified
[raw source archive](supplier-source-inputs.zip) preserve 38 files from the
three local Graft packages plus its root manifest, lock and toolchain. This is
required because Turnstone's source scanner cannot fingerprint a sibling
working copy. The supplier's six modified files are uncommitted. Its focused
Windows/wgpu-30 adapter check passed; that is separate from its published CI.

The [root build](build-result.json) passed in 2m51s at one job and BelowNormal
priority. Turnstone's `all3-sync-control` source label preserves 176 root
inputs and executable SHA-256
`ddfd7939c8ef5b761cfd127f2d83dfb9afe03f7ca3c5bb3da13fee241d9f682b`.
The root uses its own previously qualified ANGLE outputs. Foreign browser
accessibility, physical input, DPI parity, the ordered U9 repin and supplier
package publication remain separate gates.

Every comparison uses a distinct receipt name and fresh application/shared
Servo profile on the same fixture origin. Native exit zero, `RESULT ok`, and
positive caches/views are necessary but do not prove pixels. Each run must
also pass the fixture pixel reviewer and direct capture inspection. The old
white reopen remains preserved as a deliberately failing pixel control.

## Completed comparison

The [comparison manifest](comparison.json) records six runs with the same
executable, fresh profiles and fixed fixture origin. Every process exits zero,
without timeout, with `RESULT ok` and zero final producers/caches/views.

| Mode | Fixture pixel passes | Direct reopened-page review |
|---|---:|---|
| Existing | 1 of 2 | Repeat reproduces white A; B remains current |
| Producer completion | 1 of 1 | Both current alpha/bravo pages |
| Normalization completion | 1 of 1 | Both current alpha/bravo pages |
| Both | 2 of 2 | Both current alpha/bravo pages |

The reviewer passes five earlier captures and rejects only reopened A in the
failed Existing repeat. The exact negative is preserved. The 66 shell, six
find and 27 chrome executions pass, with one overlap: 98 distinct tests and
99 passing executions. This bounded sample supports a diagnostic comparison;
it does not establish visual causality or qualify a default production fix.

The [restoration record](restoration.json) verifies all 175 prior consumer
candidate inputs restored exactly from the raw archive. Only the owned
temporary `.cargo/config.toml` was removed. The complete experimental host
source and supplier source remain archived; diagnostic configuration is not
silently retained in the primary checkout. Final U9 adoption and default B3
reopen correctness remain held.
