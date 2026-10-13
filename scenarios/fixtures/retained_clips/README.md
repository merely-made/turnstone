# Native selected-content fixtures

These exports come from the first Fleece transfer qualification in Mere at
`74c1d810c4cb983cd8362ccfb99014543e9900a0`, under
`crates/fleece/examples/native_clip_transfer.rs`. Knot's native exporter is
published at `21e1b0f6f739ef023fc71152fb1f8ce80354a3e2`; Redshank's exporter is
published through Woodshed `961731ace9ae9e687bdf8d40a032f2e1fbbf3321`.

`knot.json` preserves a UTF-8 byte-span selection of `café` from an authored
document. `redshank.json` preserves the original timed annotation and recording
representation evidence. `voice.wav` is an authored 500 ms, mono, 8 kHz,
16-bit PCM 440 Hz tone. It qualifies decoder/output-clock behavior rather than
human speech or acoustic quality. These fixtures contain synthetic content and
are covered by the repository's MPL-2.0 license.

Run `cargo run --locked --example retained_clip_packages -- <new-output-dir>`
to produce three selected packages and their receiving addresses. The example
retains private Knot backing on its source, exports only the selected body,
and exits before the receiving host starts. The text-note package substitutes
an authored text body through Redshank's typed export contract. The original
recording URL is evidence; it is not a fetch or playback instruction.

The receiver scenarios use the normal Action/effect path. Import is a host
gesture; the addresses reopen local retained content. See
`design_docs/2026-10-11_retained_clip_receiver_plan.md` for host boundaries and
`docs/receipts/2026-10-11_retained_clip_receiver/README.md` for execution evidence.
