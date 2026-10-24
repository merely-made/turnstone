# Shared Genet text-input publication receiving, 2026-10-24

Genet now publishes current native editor metadata through the shared Webcue
control plane. Turnstone reads the observation through Pelt's admitted,
prepared appearance and supplies physical candidate-area geometry to its primary
and lens native windows after frame preparation.

- **The observation has one owner.** The renderer-free document-session API owns
  `SessionTextInputState` and its logical viewport rectangle. Webcue reexports
  those types; native Genet and Inker publish them independently of accessibility
  snapshot publication. The observation carries input hints and UTF-16 selection,
  without field values or document/native identities.
- **Layout and placement have separate coordinates.** Genet applies document
  scroll and page zoom. Turnstone adds the current placement and applies display
  scale once. Pool custody rejects geometry prepared for another appearance,
  even when both appearances retain the same document receiver.
- **Revocation survives a busy host.** Live observations reserve their clearing
  event. Edits, invalid layout, hiding and retirement clear old geometry. Ignored
  input retains an unchanged current caret. A legacy frame-only consumer keeps
  rendering when metadata publication encounters backpressure; polling can retry
  the current observation, and unclaimed old observations expire on replacement.
- **The dependency cohort also requires browser navigation policy.** Cambium's
  generic navigation API no longer supplies browser scope identities. Turnstone
  opts into Mere's browser policy, passes real breadcrumb routes, and validates
  persisted places and anchors through the generic history API.
- **The evidence is programmatic.** The linked logs bind tests to the source and
  package cohort in [receipt.json](receipt.json). Native hardware tests use Metal on AMD Radeon
  Pro Vega 56; the host geometry test uses real retained Pelt/Genet documents for
  primary and lens window identities. It does not open an OS candidate popup.
  The earlier physical Typing Booster receipt remains unchanged.
- **Qualification remains bounded.** This run does not requalify physical CJK
  input methods, human accessibility use, Servo IME or candidate popup placement.
  A ThinkPad Linux/Vulkan rerun could not start while Djinn's mesh SSH identity
  remained locked. Source inspection also retains a separate native password
  masking/index-projection cue: classification and value exclusion are checked,
  but password offset-to-glyph accuracy is not established by this receipt.

Published packages are `document-session-api 0.1.1`, `webcue 0.1.2` and
`wgpu-cue 0.1.3`. [published-packages.json](published-packages.json) records registry archive checksums,
clean source revisions, Rust-source equality and manifest-source equality.

The prior physical evidence is retained in
[its original receipt](../cue_ime_linux_20261024/README.md). The standalone SDK
proof is retained [in Wgpu Cue](https://github.com/merely-made/wgpu-cue/blob/main/docs/2026-10-24-text-input.md).
[SHA256SUMS](SHA256SUMS) covers every other file in this directory.
