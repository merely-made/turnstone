# Browser consumer integration with current main, 2026-10-06

Status: qualified for the bounded Windows primary-Workbench browser slice.

Browser commit `a383cdd4fd690d73927d01db9ec21c3bfd975150` completes the four
Windows native scenarios in the frozen
[browser receipt](../browser_scry_windows_20261006/README.md). While preparing
its push, current main had advanced to
`97e8e49667685346e4898a859364d37757f9996e`, integrating the stable Burn graph.
This receipt qualifies their combination separately. It preserves the upstream
Mere `3d1cdacc90aa6a0736154d841223a3ae29e23aa5` and Woodshed
`9e982b88bf57e41ef4fa846c66ffdd4f05beb91d` pins and root Burn patches, together
with the browser's opt-in Weld adapter and immutable Scry queue repair.

Only Cargo.toml and Cargo.lock differ between the qualified browser snapshot
and this integration's application source. The Inker and surface-api trees
are unchanged between Mere `bd5912fb` and `3d1cdacc`; the empty
`mere-browser-seams.diff` records the scoped Git comparison. This dependency
integration does not apply the held graph-semantics consumer patch.

The first `--offline --locked` build failed because the new Git pins were not
cached. `build.log` preserves that failure. The subsequent locked build fetches
the real immutable sources; no path override or lock regeneration is used.
The target remains `C:/t/cargo-targets/turnstone` and the existing CEF SDK is
151.3.24. The direct executable explicitly uses the trusted-content unsandboxed
route; this receipt does not qualify the bootstrap route.

The combined production build passes with `--locked --features scry,weld`.
The cached-source test runs then use `--offline --locked` with the same
features: 60 shell tests, five document-find tests and 18 retained chrome tests
pass serially. These compile the real library test target against the merged
dependency graph. They are focused consumer checks, not a full-suite rerun.

`source-manifest.json` freezes application Rust, Cargo files, scenarios,
fixture HTML/CSS/JS and executable SHA-256
`8dfbcbfe4ca06960d8e61febdd574ad73c23f4108cc043c5c53df29240450175`.
The final collection checks that only Cargo.toml/Cargo.lock differ from the
earlier qualification inputs and verifies every source/executable fingerprint.

All four native runs report RESULT ok on that executable: two-page Weld input,
find/requested zoom/teardown; two-page Scry input/current pixels, cookies,
resize/switch/reconstruction/close; Scry exact-profile separate-process
restart; and Weld's real permission denial/callback/result-page/teardown.
The permission fixture server also reports RESULT ok. Direct capture review
confirms current populated pages, independent input, actual find/zoom outcomes,
the exact-origin permission card and callback result, and cleared closed views.
The Scry close snapshot has zero producers, zero cached frames and no
per-producer importers; the shared factory root is not claimed destroyed.

`facts.json` collects actual test counts and native results.
`visual-review.json` names the inspected composed-frame PNGs.
`artifact-manifest.json` hashes the retained evidence bytes, excluding synthetic
profiles and the manifest itself. The prior receipt and failed controls remain
unchanged. Both sets of profiles remain available for exact-snapshot evidence.

The earlier broad suite remains scoped to the earlier source: it had one
parallel network sync-round failure that passed in isolation. This integration
does not reclassify it as a clean suite. Physical keyboard, supplementary
Unicode, OS IME, cross-engine DPI parity, successful geolocation after Grant,
Servo B3 and new package release gates remain open as described in the prior
receipt. Weld requested zoom stays Partial.

No isolated Cargo home or worktree is created. Ignored synthetic profiles
belong to this receipt and are retained for exact-profile process restart.
Unrelated `.github/` work and shared builds are preserved. Reproduction uses
the same loopback origins, scenarios and runner scripts as the prior receipt;
existing capture directories are refused rather than overwritten.
