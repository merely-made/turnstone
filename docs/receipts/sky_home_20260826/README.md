# Sky home T5a headed receipt

This receipt proves Turnstone's first retained astronomy consumer against
Turquet 0.13.0 at revision
`bc3c454f755d0bfd70ab48bd9556a1cda2213d41` and the compatible Genet
AccessKit seam at revision `4e60f931a8d8caa29530494b2103e4c744c8ccf9`.

The fresh-profile scenario opened the versioned `turnstone.sky` pane, verified
the Boston 2024-04-08 reference receipt, activated the semantic **Next day**
button, and verified the independently calculated 2024-04-09 projection. The
opening pane source remained immutable while the retained pane replaced its
applied request, timeline, and receipt together.

The exact receipt digests are:

- 2024-04-08:
  `caff8371d348ba141397a8185e291c533c6ab12d4fe85f9ce3be797707ce411d`
- 2024-04-09:
  `74883b5db9fa959cccdeb69744da4a99baec5bfd55bade77426cfe6b0d9c450f`

## Artifacts

- `sky_home.png`: visually inspected 1024 by 600 capture, 211754 bytes, SHA-256
  `7c0a8cfe768b7f37d00f919a611db3635d62f1b5a407be7ba35b43fa725b3d49`.
- `scenario.done`: shared scenario-driver result and capture log.
- `acceptance.done`: stable post-check record for the decoded PNG and exact
  source identities.

## Verification

- Seven focused Sky surface tests passed against the exact Turquet source.
- Seven generic contributed-surface tests passed, including semantic Probe,
  ordinary Cambium click delivery, and AccessKit action routing.
- The exact-source Turnstone binary built successfully.
- `scenarios/run_sky_acceptance.ps1` required `RESULT ok`, a zero process exit,
  a PNG signature and IHDR chunk, and nonzero dimensions before reporting
  success.
- The headed scenario resolved **Next day** through the projected AccessKit
  tree before activating the same retained control through Probe.

The wider library run passed 378 tests with 5 ignored and 2 unrelated
failures. The committed G3 HTML receipt mismatch was solely LF versus CRLF in
the isolated verifier clone; the ordinary checkout and committed blob use LF.
The partition-heal transport test timed out in the full run and again alone.
Neither failure overlaps the Sky or contributed-surface paths, so they are
recorded here rather than represented as a green whole-library gate.

This receipt proves semantic DOM and keyboard-ready controls through Cambium
and Probe, plus provider-neutral projection into Turnstone's AccessKit tree.
It does not claim a manual Narrator walk, restart persistence for the
contributed source, scroll-aware Probe coordinates, or stable Probe selection
among multiple contributed panes.
