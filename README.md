# turnstone

Turnstone is a local-first browser for shared, addressable places where people
talk, author, publish, and bring other webs into view. Its canvas relates
people, conversations, shared places, documents, services, and remote
resources in one durable graph. HTTP and the smolweb protocols are resource
and exchange lanes within that peer web rather than the product's boundary.

Turnstone is built on
[mere](https://github.com/merely-made/mere), the graph datalake library and
repository home of the peer-domain packages it composes. Mere supplies graph
truth and portable projections; Murm supplies invited exchange; Gemot governs
durable moots; the Commons profile supplies shared graph and channel
convergence; Knot owns authored documents; and Genet presents local and remote
content. 

Be disclaimed: I use AI to develop this entire stack, as a tool, carefully, and with review.

## Build and run

Windows native builds need the MSVC C++ build tools, Windows SDK, CMake and
Ninja. The CEF dependency downloads its matching binary distribution when needed;
set `CEF_PATH` to an existing matching distribution to reuse it. This is a native
SDK input, separate from Cargo's source redirects.

```sh
cargo run     # the turnstone window
cargo test    # unit tests
```

Turnstone pulls `mere` and the genet engine family as git dependencies; a plain
`cargo build` fetches them. Headed self-drive receipts live under `scenarios/`.
The committed `Cargo.lock` records the portable graph on Rust 1.98.1. Run
`cargo check --workspace --locked`, or use Python 3.11+ to check configuration,
package provenance and lock preservation with `python scripts/cargo_mode.py verify`.

For sibling development, copy `.cargo/config.toml.example` to
`.cargo/config.local.toml` and adjust the paths. Existing users run
`python scripts/cargo_mode.py setup` once: current locks are preserved and ignored
automatic configs become opt-in local configs. Run
`python scripts/cargo_mode.py local check --workspace` for sibling resolution;
it requires Cargo 1.97+ and uses `.cargo/local/Cargo.lock`. Bare Cargo and editor
metadata use the portable graph; configure editor checks to use the launcher
when working across repositories. Local config and locks are never committed.

Advance Git pins as tested integration sets, rather than chasing every sibling
commit. A local build and a redirect-free locked build are separate checks.

The September 22 portable set is Mere `0e031fa5`, Genet `99769450`, Knot
`8610058b` and Woodshed `f5a66493`. Knot and Redshank moved to that Mere and
Genet first, so Knot's publishing/projection interfaces and Redshank's Cambium
surface interface cross into Turnstone as one copy of each Mere type; the
September 20 attempt failed because they still carried Mere `ca798151`.
Sibling mode uses current local Mere/Genet with a separate lock.
The portable pins must advance as a tested set covering those interfaces.

## Status

Working reference host. Turnstone obviated mere's former `meerkat` crate on
2026-07-18: the behavioral deletion matrix went green and meerkat left mere's
tree, so the browser host now lives here as its own binary over the `mere`
library.

What runs today: the graph canvas (pan / zoom / isometric, deterministic
layout strategies); a summonable omnibar (find / go / actions lanes);
back, forward, reload; live web content on two engine lanes (the genet stylo
lane and the clean-room `genet.livery` lane) with a per-node viewer override;
retargeting panes (Roster, Trail, Gloss, Inspector, Apparatus) and a
platen-tiled Workbench; multi-window lenses with identity-preserving pane and
tile tear-out; and multi-session (`sessions/<id>/` with a switcher and
restart restore). Every capability carries a self-driving scenario receipt
(the shared taproot driver) plus an accessibility projection.

The peer-web product spine is not yet wired end to end. Personae identity,
Murm, Gemot, shared graph/chat convergence, Knot communal sync, and Retinue
carriage have separate executable receipts; Turnstone still lacks the live
place port that composes them. The bounded integration plan is the
[peer-web reframe](design_docs/2026-07-28_turnstone_peer_web_reframe.md).

The live plan is `design_docs/2026-07-10_turnstone_architecture_plan.md`; the
founding brief is `design_docs/2026-07-08_turnstone_founding.md`.

## Screenshots

<p align="center">
  <img src="assets/screenshots/gloss-minimap.png" alt="Turnstone graph canvas beside its Gloss minimap" width="900"><br>
  <sub>Gloss keeps the graph's wider shape visible while the canvas stays focused on the current node.</sub>
</p>

<p align="center">
  <img src="assets/screenshots/workbench-split.png" alt="Turnstone Workbench with two graph nodes rendered as tiled web documents" width="900"><br>
  <sub>The Workbench tiles graph nodes into live document surfaces beside the canvas.</sub>
</p>

## Graphshell endpoint

Turnstone exposes its first local Graphshell projection through the library's
`remote_projection` adapter. Mere cartography maps the live graph into a
sceno spiral and routed relations; Graphshell resolves separately
transferred cards and returns advertised intents through Servitor. The G3
receipt and exact acceptance boundary are recorded in
`docs/2026-07-22_g3_graphshell_endpoint_receipt.md`.

## License

MPL-2.0 (see LICENSE).
