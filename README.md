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

The local Windows system-webview implementation is optional:
`cargo run --features scry`. It requires the WebView2 runtime and a D3D12 host;
construction failures are reported by the browser. Two-page native qualification
records cookie retention and current captured pixels in the
[supplier integration receipt](docs/receipts/browser_supplier_integration_20261006/README.md).

The published U14 checkpoint (`4e217ef`) pins Mere `edf175f9`, Genet
`679d8314`, Knot `211ff57a`, Woodshed `24f196f4` and Retinue `fa4f9250`.
The [October 7 repin](design_docs/2026-08-25_browser_surface_implementation_plan.md)
updates upstream Servo to 0.7 and CEF to `154.5.0+154.0.34`, then qualifies
the current coordinated family: Mere `57b4893d`, Genet `965b64e2`, Knot
`0096591a`, and Redshank/Woodshed `82271df2`. The browser suppliers are Scry
`2c3ebd24`, Weld `c4dd593b`, and Graft `bb48281b`; upstream Servo is
`aac43a3f` (0.7.0). The
[current-family receipt](docs/receipts/browser_family_20261007/README.md)
records source identity, compatibility changes and exact qualification scope.
The family then advanced twice. The browser lane's foreign-accessibility
repin (`fc52533`) moved Mere to `f1d169c7` and Knot to `14cd06e`. SC step 4
(2026-10-08) moved it to Mere `3ded2cd7`, Genet `965b64e2`, Knot `6e66f1ab`
and Redshank/Woodshed `3ef71040`, bringing in Pelt's routed browsing
controller (`pelt-core`) and the sans-IO page load (`page-load`). The
Scenograph C2 and vault lock repin (2026-10-08) moved it to Mere `463c8d40`,
Genet `15713014`, Knot `5bef84c0` and Redshank/Woodshed `52fbe468`: the
command menu stores Cambium's own choices under the stack's shared command
ids, and a locked personae vault leaves the profile identity pending rather
than replaced. SC step 4's pass B (2026-10-08) moved it to Mere `0357b286`,
Knot `ee0512d0` and Redshank/Woodshed `2f8ac103`, Genet unchanged: each
node's web surface is the surface lane of its Pelt content.
Knot and Redshank
share the Mere/Genet surface family with Turnstone so retained-surface,
publishing and projection interfaces have one source identity.
Sibling mode uses current local Mere/Genet with a separate lock.
The portable pins must advance as a tested set covering those interfaces.

## Status

Working reference host. Turnstone obviated mere's former `meerkat` crate on
2026-07-18: the behavioral deletion matrix went green and meerkat left mere's
tree, so the browser host now lives here as its own binary over the `mere`
library.

What runs today: the graph canvas (pan / zoom / isometric, deterministic
layout strategies); a summonable omnibar (find / go / actions lanes);
back, forward, reload; clean-room `genet.livery`, Reader and native smolweb
document lanes, plus optional Windows `weld.chromium`, with a per-node viewer
override; retargeting panes (Roster, Trail, Gloss and Inspector) and a
platen-tiled Workbench; multi-window lenses with identity-preserving pane and
tile tear-out; and multi-session (`sessions/<id>/` with a switcher and
restart restore). Dated scenario receipts record their exact native behavior
and accessibility scope; broader human accessibility acceptance remains open.

The live place port composes Personae, Gemot and shared graph/chat convergence.
Recorded native place scenarios cover shared addresses, reader write refusal,
reconnect and a founder-held Knot document edited through its projection.
That document path depends on its holder being online. The exact scope is in
the [place port plan](design_docs/2026-07-28_turnstone_place_port_plan.md).

Current browser work follows the
[user-agent taxonomy](design_docs/2026-08-03_user_agent_taxonomy_plan.md):
registry-derived engine choices and optional Windows Scry, Weld and
Servo/Graft hosts on the existing wgpu device. Scry and Weld use per-node
profiles; Servo shares one explicitly named, configurable process profile
across its views. The [integration receipt](docs/receipts/browser_supplier_integration_20261006/README.md)
records scoped Scry/Weld input, persistence, permission and teardown controls,
and mixed-runtime pixels. Servo's default reopen can still produce a white
frame. Those captures qualify their recorded binaries; the upstream repin
requires fresh native checks. Current-family resize probes also expose stale
Servo pixels after the page has received its new viewport. Explicit GPU
completion diagnostics have positive and negative controls; reliable default
ordering remains a Graft release gate. Foreign accessibility, OS IME, lenses and
native rehosting remain separate gates.

The portable source includes two published Mozilla private ICU packages with
three dependency ranges widened; their Rust, data and license bytes are
preserved. An exact official ICU CAPI release source isolates its FFI-facing
2.1 family from Genet's newer Rust ICU family. Provenance, resolver controls
and seven page-native Intl/normalization probes are in the current receipt.

The local October engine-picker patch derives choices from actual registries,
uses human labels, and keeps unavailable saved pins visible with reasons. Its
[qualification receipt](docs/receipts/browser_engine_inventory_20261005/README.md)
records default/optional-Weld tests and the native picker scenario.

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
