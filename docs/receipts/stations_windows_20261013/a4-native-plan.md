# A4 native qualification plan

This file describes the native fixtures and acceptance boundaries. Actual final
outcomes are recorded in `published-final-native-qualification.json`: seventeen
affected normal-source tests and the binary build pass, followed by fourteen
headed scenarios and forty primary/lens captures on published Mere77fd/Genetb6/
Netrender6a. Preliminary local-path results retain their separate authority.

## CLI

Use the qualified `C:/t/cargo-targets/turnstone/debug/turnstone.exe`. The scenario
CLI is selected by environment variables, not a `--scenario` flag. Give each run
private profile and Mere store paths beneath this receipt directory:

```powershell
$env:TURNSTONE_ROOT = "$PWD/docs/receipts/stations_windows_20261013/native/a4/profile"
$env:MERE_ROOT = "$PWD/docs/receipts/stations_windows_20261013/native/a4/mere"
$env:TURNSTONE_CAPTURE_DIR = "$PWD/docs/receipts/stations_windows_20261013/native/a4/captures"
$env:TURNSTONE_SCENARIO = "$PWD/scenarios/a4_inline_components.scn"
& C:/t/cargo-targets/turnstone/debug/turnstone.exe
```

Record executable SHA256, Turnstone source commit plus owned source hashes,
Cargo.lock hash and the actual published Mere/Genet supplier revisions. Inspect
the emitted `scenario.done` and captures, then record outcomes independently.

## Required cases

- `a4_inline_components.scn`: actual inline Roster body, retained Links tab through
  tree to float to lens to primary, current published body accessibility targets
  in each receiving window, header focus entry/exit, body arrow keys and captures.
- `a4_native_overlap.scn` with `TURNSTONE_THEME_SCENARIO=frame_alpha_workshop.scn`
  and private `TURNSTONE_THEME_LIBRARY`: authored `opacity:0.55`, orange alpha
  shadow, inline float above the actual Canvas producer, then a nearer Canvas
  float above the retained Roster owner, including resize and settled captures.
  Verify the shared opacity applies once to frame and native body together;
  lower text/graph pixels remain visible through translucent nearer content;
  the shadow crosses both source types without being erased; moving/resizing
  leaves the authored background without stale prior-window pixels. Retained
  Links state and actual native image staging count must remain present.
- `a5_floating_pane.scn`, `frame_stations.scn`, `frame_stations_reopen.scn`:
  floating geometry, station persistence and reopening in a fresh process.
- `frame_controls_keyboard.scn`, `frame_controls_accessibility.scn`,
  `frame_controls_reopen.scn`, `frame_lens_accessibility.scn`: current-window
  header/divider/tab/close actions and persistence.
- `frame_lens_scroll.scn`: actual receiving-window pane wheel delivery, with
  before, scrolled and settled lens captures. The existing fade clock is
  unchanged. The final-source immediate fixture's next capture dispatch was
  1396 ms after wheel delivery; the final published run measured 1301 ms. Both
  exceed its 900 ms hold and 250 ms fade. Actual
  body movement and the scoped redraw continuation control are qualified;
  those captures do not qualify early scrollbar opacity pixels.
- `frame_appearance.scn` with `TURNSTONE_THEME_SCENARIO=frame_fonts_workshop.scn`,
  then `frame_appearance_reopen.scn` on that same private profile: authored frame
  stylesheet/font changes, narrow resize and reopened selection. Use private
  `TURNSTONE_THEME_LIBRARY` for the workshop output.
- `frame_appearance_missing.scn` with `frame_fonts_missing_workshop.scn`, then
  `frame_appearance_missing_reopen.scn`: unavailable font fallback and reopening.

The in-house bodies must be painted once inside the shared frame, with correct
foreground/background and clipping through floats. Graph/document producers
remain owned native producers, staged as ordinary image commands at their exact
retained appearance apertures. One shared window scene is rasterized, so CSS
clips, transform/layer ordering and group alpha surround actual source images.
Acquisition/fence, producer/session input and accessibility custody remain at
the original owners. Classic Vello output is straight alpha; WebView/CEF and
Servo/WebRender imports preserve explicit premultiplied conventions. sRGB
hardware-decoding import views are refused until encoded staging is supplied.
The current slot placement planner uses translated physical-pixel apertures;
this fixture does not qualify arbitrary authored transform inverses. Captures must be
nonblank and visually reviewed; `RESULT ok` alone does not establish visual
acceptance. Window-scoped fixture keys enter after OS admission and deliberately
do not activate a physical window. Queued accessibility fixtures use the actual
published tree and action queue, but are not a human AT receipt.

## Focused automated gates

- Cambium `retained_mount`: real owner state and DOM identity across destination
  mount/source teardown, source-first withdrawal followed by actual collection
  and remount, refusal to mount a foreign document, and final runtime teardown
  before releasing the owner root. The keyboard control also verifies that
  stock tab arrows explicitly request actual focus and programmatic sync does
  not claim it. The earlier five-test Genet 561 gate in
  `retained-mount-561-roving-tests.log` predates the final request contract.
  Eleven current tests additionally cover custom cancellation and selection,
  foreign-host refusal, reused-event request retirement, explicit same-node
  focus priority and target removal. All eleven passed against locked Genet
  19947; see `retained-mount-19947-stock-request-tests.log` and the source hashes
  in `shared-19947-mount-polling-tests.json`. Those model tests alone did not
  close native A4; the final published headed receipt above now records the
  bounded consumer acceptance.
- Pelt's native mailbox and retained-document polling controls both passed
  against locked Genet 19947. The observation is read-only, follows admitted
  visible surface placement between windows. Native Surface observation excludes
  retained documents; the separate final pending-work observer and its 25 pool
  controls qualify retained document deadlines without invalidating routes.
- `cambium_pane::tests::inline_roster_uses_shared_fragments_and_preserves_owner_across_windows`:
  actual shared fragment hit testing, float continuity and destination-first move.
- `share_reader_pane::tests::inline_ticket_field_routes_keys_composition_and_caret_in_shared_layout`:
  controlled keys/composition, real editable semantics and shared caret bounds.
- `outer_frame::native_aperture_tests::identical_source_rectangles_keep_exact_appearance_owners`:
  equal source/geometry resolves exact appearance roles and refuses duplicates.
- `shell::native_composition::tests::allocated_producers_preserve_custody_and_rootstock_range`:
  live appearance custody, non-aliasing allocation and actual renderer key domain.
- `scenario::tests::inline_component_station_fixture_uses_supported_grammar`:
  prepared diagnostic grammar and supported body action vocabulary.

Physical platform activation, keyboard/IME behavior, human AT, and a real monitor
DPI transition remain separate acceptance gates. Resize, synthetic committed text,
queued accessibility, and CPU geometry do not close those gates.

## Selected raster diagnostics

`TURNSTONE_A4_DIAGNOSTICS=1` adds `.scene.json` and `.raw.png` beside each
primary capture. The JSON is the exact emitted shared scene. The raw PNG reads
the same raster texture before the final sampling/composition pass, using an
owned asynchronous copy polled on later event-loop turns. These diagnostics
locate native composition failures; they do not establish acceptance by
themselves. Earlier receipt-local unscoped diagnostic fixtures remain preliminary;
the final published runs use canonical scoped selectors with the adopted Taproot
repair preserved in Genet b6.
Owned scene producers additionally write `.source-<key>.png` and `.sources.json`.
Those pixels come from their actual raster textures before external-image
staging. Source metadata records window/session, SurfaceId, image/producer key,
content/device generation, dimensions, encoding format, declared alpha,
actual adapter/backend and native-window/raster scale.
Imported view-only producers remain under their importer ownership and are not
reconstructed from final window pixels for this diagnostic.
