# Redshank episode surface: Turnstone as the port's second host

**Status (2026-09-14): LANDED, with five named gates.** Turnstone mounts
Redshank's own compact Player/Capture dock as an episode tile, routes podcast
enclosures to it, and projects one library item, its progress, and its timed
notes into the graph. The listening model, its durable store, and the one
Firewheel/CPAL output authority are app-owned and shared by every tile.

This is Phase 7 of the port plan, which lives in the Woodshed repository at
`ports/redshank` and remains the product authority:
`repos/woodshed/design_docs/2026-09-01_listening_annotation_port_plan.md`
("Phase 7: Turnstone as second host"), sequenced by
`repos/woodshed/design_docs/2026-09-13_redshank_gui_implementation_plan.md`
("Pass 2 lanes" → "Lane T"). Nothing here restates the port's own design; this
document records only what Turnstone owns.

## The seam

Redshank is consumed as four git dependencies on `woodshed.git`:
`redshank-surfaces`, `redshank-model`, `redshank-playback`, `redshank-storage`.
They live in a nested Cargo workspace (`ports/redshank`) excluded from
woodshed's root workspace; cargo's git source reads packages anywhere in a
repository, honouring `workspace = true` inheritance from the nested root, so
the nesting is a dependency detail rather than a second source. That was
verified directly, against a pushed revision, before the pin was written.

Four new files carry the slice. Existing files are touched at one point
each: `feed.rs` gains the episode projection and the audio judgement,
`content_classes.rs` the two profile-facet schemas, `node_arms.rs` the routing
arm, `feed_arms.rs` the progress tick, `session_lifecycle.rs` the reopen, and
`shell/mod.rs` the registration, the live output, and the runtime's wake.

| File | Owns |
|---|---|
| `src/redshank_episode_surface.rs` | The provider: pane kind `turnstone.redshank-episode`, schema `redshank.episode.v1`, the versioned source payload, and `RedshankDocks` — every mounted dock by library identity. |
| `src/redshank_host.rs` | The ONE Redshank authority: the model, the `JsonDirectoryStore` under `<session_dir>/redshank/`, the `PlaybackRuntime`, the command application, and the compact projection. |
| `src/app/redshank_arms.rs` | What belongs to `App`: enclosure routing, the dock command loop's app half, and the graph projection of an item, its progress, and its notes. |
| `src/shell/events.rs` | `Shell::pump_redshank`, one turn per event batch beside `App::tick`. |

The provider is registered in `Shell::new` beside the Knot document, Sky, and
Distillery providers, and is the fourth independent consumer of the
contributed-surface seam — the first from outside Mere's own workspace. The
descriptor, the stylesheet, and the erased session all come from
`redshank_surfaces::surface_api`; there is no Turnstone copy of the dock.

### Why the surface crate grew a `surface_api` module

`RetainedSurfaceSession` erases the product runner completely, so a host has no
way to reach the dock's state. The dock needs two things crossing that
boundary every frame: the host's projection in, and the listener's commands
out. `redshank-surfaces` therefore publishes a `CompactDock` handle — a shared
projection cell and a shared command queue — and a session that pumps both on
its viewport hook and after every dispatch. `RunnerSurfaceSession` could not
carry it: its action type is `()` (the dock queues commands in its state rather
than bubbling them) and its viewport closure fires only on a size change. The
session is otherwise the same fourteen one-line delegations.

### One decision point for routing

`App::open_address` asks `redshank_episode_for_address` first. A subscribed
feed entry answers with its show, GUID, artwork, and bound graph member; a bare
`audio/mpeg`, `audio/mp4`, `audio/aac`, `.mp3`, `.m4a` or `.aac` address
answers with itself. Anything else falls through to the reader untouched.
`crate::feed::is_podcast_audio` is the only place that judgement is made.

### Derived fields, durable stores

Progress rides the episode member as a `redshank.progress` profile facet and a
note's anchor rides its own node as `redshank.note`, both registered in
`content_classes.rs` so the inspector shows real fields. Neither is durable:
node facets are not written to the session graph, exactly as the built-in
web-page and note facets are not. The durable pair is the Redshank model
(`<session_dir>/redshank/`) and the feed store (`feed_subscriptions.json`),
which between them name the item, its feed and GUID, and the member the feed
bound. `App::reconcile_redshank_fields` re-mints the fields on session
adoption. Re-minted, never repaired.

The note itself is an ordinary graph node at `mere://redshank/note/<id>`,
titled with its anchor in source time, bodied with its text, tagged
`redshank-note`, and related to its episode by `SemanticSubKind::Quotes`. That
node is session-graph truth and survives a restart on its own.

## Done-conditions

| Port plan condition | State |
|---|---|
| Turnstone opens a podcast enclosure through handler routing | **Met.** `opening_an_enclosure_yields_a_redshank_episode_pane`. |
| The port's compact surface appears without a copied Turnstone implementation | **Met.** The descriptor, stylesheet and session are `redshank-surfaces`'; the provider is an adapter. |
| One library item, progress update, and timed note project into the graph and reopen after restart | **Met.** `a_note_and_progress_project_into_the_graph_and_reopen_after_a_restart`, a two-`App` test over one session directory. |
| The embedded path uses Turnstone's existing device and network authorities | **Device met, network NOT met.** See the fetch gate below. |
| Standalone and embedded conformance tests run against the same action and snapshot contract | **Met.** `the_dock_emits_the_same_commands_through_turnstones_registry` re-runs `surfaces/src/dock.rs`'s own assertions against a session admitted through `SurfaceProviderRegistry`. |

## Open gates

1. **The fetch path.** `redshank-playback` streams HTTP(S) ranges itself, with
   `ureq` over `rustls`, inside its own worker thread. Turnstone's network
   authority is `mere-fetch`. This is the one place the embedded path does not
   reuse a Turnstone authority, and the port plan already lists it. The
   alternative is a host-blob source: Turnstone's download lane fetches the
   enclosure to a session-owned blob, the model's `MediaSource::HostBlob`
   names it, and the runtime opens a local file — which needs a blob store
   keyed by enclosure identity, range-aware progressive download so playback
   starts before the file completes, and a cache budget that agrees with
   Redshank's own. Not built; the honest report is that a Redshank tile makes
   its own outbound requests.
2. **No microphone.** Turnstone supplies no capture adapter, so the projection
   keeps `voice_capture_available` false and the dock disables Voice honestly.
   A voice command that arrives anyway is refused in words rather than
   silently dropped.
3. **The woodshed pin is provisional.** The manifest names woodshed
   `c7f68dcd` — `origin/main` — because cargo loads a git source even when a
   `[patch]` redirects every package in it, so the unpushed merge `882dc754`
   this slice was written against cannot be named at all. The gitignored
   `.cargo/config.toml` redirects the four packages to the sibling checkout,
   so what compiles here is the 882dc754 source. Move the pin the moment that
   merge is pushed.
4. **The Mere pin needs knot-editor to move.** Turnstone is repinned to mere
   `1009f02d` so it can agree with Redshank on
   `mere_surface_api::SurfaceDescriptor` and
   `cambium::RetainedSurfaceSession`. `knot-editor` `5c763e2e` still pins mere
   `3675a352`, and cargo's SourceId includes the git ref, so a clean checkout
   resolves two copies of every shared Mere package and fails with thirty-six
   "expected `SurfaceDescriptor`, found a different `SurfaceDescriptor`"-shaped
   errors. A machine-local `[patch."…/mere.git"]` table collapses both refs
   onto the sibling checkout, which is how this tree builds today. The repair
   is upstream: realign `knot-editor` to mere `1009f02d` and repin it here, in
   one commit, the way `6ec391f` moved mere and knot-editor together.
5. **No headed receipt.** Everything above is headless. A tile rendered in a
   real window, with audio, is not yet captured.

## Verification

Run from inside the repository, so its `.cargo/config.toml` and target
directory apply:

```
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

`cargo test --workspace`, 2026-09-14: **525 passed, 0 failed, 9 ignored**,
seven of them this slice's.

`cargo clippy --all-targets -- -D warnings` does **not** pass, and did not
before this slice either: it stops with "could not compile `turnstone` (lib)
due to 140 previous errors" and "(lib test) due to 123 previous errors", almost
all unused imports left by `c0fcf36`. Zero of them are in the files this slice
added or the lines it changed — `cargo clippy --all-targets --message-format
short | grep redshank` returns nothing. Clearing that debt is its own slice;
until then the honest gate here is `cargo check --workspace --all-targets`,
whose warning set is unchanged by this work.
