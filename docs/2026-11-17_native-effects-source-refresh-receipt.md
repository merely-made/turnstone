# Native effects source refresh receipt

This receipt records Turnstone's portable native consumer against the qualified
Genet and Netrender source revisions. The integration commit before this receipt
was `38eab344a4e8530d65fa966eb44658a4e8f758fb` (`Document portable graphics
source refresh`).

## Resolved sources

- Genet: `3dc6a07c01cafc842ccf9b0ed3a657f0c7428332`
- Netrender: `7060634fb93c6df5b43c4971e03ccaef0b2b7feb`
- Portable ignored `Cargo.lock` SHA-256: `266F983B8AD25DFE17D74076A5A6A624A9BADBF1B57BF0C66AD1C6F1E17A9444`
- The root lock was seeded from the base commit's resolved note, then updated
  only with the paired Genet and Netrender revisions. Other source package
  identities match the base graph. Cargo's resolver changed three already
  locked version edges: `data-encoding-macro-internal` uses `syn 1.0.109`,
  `prost-derive` uses `itertools 0.12.1`, and `snafu-derive` uses `heck 0.4.1`.
- Git ignores the generated lock. The commit's resolved note is the durable
  portable graph record; inspect it with `house resolved show <commit>`.

## Bounded gates

Run from the portable worktree with `CARGO_BUILD_BUILD_DIR=C:\t\cargo-targets\turnstone`:

- `cargo check -p turnstone --bin turnstone --locked --offline`: passed on
  2026-11-17.
- `cargo test -p turnstone --lib shared_primary_and_lens_plans_do_not_paint_discarded_frame_scenes --locked --offline`:
  passed, 1 test.
- `cargo test -p turnstone --lib outer_frame::native_aperture_tests::the_final_floating_canvas_keeps_one_actual_native_aperture --locked --offline`:
  passed, 1 test.

These are a compile gate and two no-service structural tests. They do not run a
headed Turnstone session, qualify native WebGL presentation, or prove a live
canvas generation refresh. Pelt's active `DocumentSession<Scene>` path lowers
content to a static scene and rasterizes it into its own texture; the active
Pelt document registry does not activate Genet's scripted external-texture
producer path. The new Genet/Netrender behavior is therefore consumed by the
Turnstone native shell graph, while live web-document canvas qualification
remains outside this receipt.

Raw command logs and the diagnostic first compile are retained under
`C:\Users\mark_\Code\testing\turnstone\receipts\css-effects-20261116`.
