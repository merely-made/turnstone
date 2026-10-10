# graphshell (port)

The reference application over [`crates/graphshell`](../../crates/graphshell).
The stack crates carry sessions; this port decides who gets one, runs the serve
loops, hosts resident endpoints, and renders projections.

Package name `graphshell`. It re-exports the stack as `graphshell::protocol`,
`graphshell::client`, and `graphshell::endpoint`.

## Modules

Always compiled:

| Module | Contents |
|---|---|
| `view` | `ProjectionReceiptView`, `ProjectionLayoutView`, `ScenePlacementView`, `SceneRelationView`, `IntentReceiptView`, `render_projection_receipt`, `render_g1_receipt`, `render_canary_html` |
| `canary` | `FixtureEndpoint`, `CanaryRun`, `CanaryError`, `run_loopback_canary` |
| `resume` | `ResumeFixtureEndpoint`, `run_resume_canary` |
| `sessions` | `SessionProjectionView`, `render_session_switch_receipt`; under `native`, `spawn_endpoint_session` and `mount_endpoint_processes` |

Under `native` (non-wasm):

| Module | Contents |
|---|---|
| `admission` | `GRAPHSHELL_DOMAIN`, `PROJECTION_SERVICE`, `CONNECT_ACTION`, `PROJECTION_PROTOCOL`, `open_session`, `admit_session`, `serves_action` |
| `carrier` | `projection_alpn`, `projection_policy`, `accept_projection_session`, `ProjectionRefusal`, `ProjectionAcceptError` |
| `network_carrier` | `projection_binding`, `dial_projection_session`, `DialError`; re-exports `NetworkCarrier` and `CarrierRuntime` |
| `session_loop` | `serve_admitted_session`, `SessionEnd`, `SessionSummary`, `SessionLoopError` |
| `session_notices` | `serve_admitted_session_notifying` |
| `lifecycle` | `SessionAuthority`, `AdmittedEndpointContext`, `BindAdmittedSession`, `Lapse`, `ScoreDenial`, `adjudicate_intent`, `apply_lapse` |
| `browser_carrier` | WebExtensions native messaging: `NATIVE_HOST_NAME`, `CHROMIUM_EXTENSION_ID`, `FIREFOX_EXTENSION_ID`, `AllowedExtensions`, `BrowserLauncher`, `BrowserChallenge` |
| `identity` | Secret-free read model: `VaultView`, `ProfileView`, `SshKeyView`, `DeviceView`, `DeviceGrantView` |
| `identity_endpoint` | `SupplementalCard`, `TransferAcceptIntentV1`, `TransferDecision`, `IdentityEndpointError` |
| `identity_projection` | Identity cards plus the signing, SSH generate, and SSH import intents |
| `policy_projection` | `PolicySettingsView`, `run_n4_policy_scenario`, `render_n4_policy_receipt` |
| `profile` | `PROFILE_ENV` (`GRAPHSHELL_PROFILE`), `selected_profile`, `default_vault_dir`, `GraphshellIdentity` |
| `native::endpoint_catalog` | `ResidentEndpointCatalog`, `ResidentEndpoint`, `ResidentEndpointRoute`, `ResidentEndpointSession` |
| `native::projection_host` | `ResidentProjectionHost`, `ServedProjection` |
| `native::personae_host` | Resident Personae authority; SSH key mutation receipts |
| `native::browser_host` | `serve_identity_native_messages`, `serve_catalog_native_messages` |
| `native::device_broker` | `DeviceSurface`, `DEVICE_ENDPOINT_ENV` (`GRAPHSHELL_DEVICE_ENDPOINT`), `configured_device_endpoint` |
| `native::identity_ui` | `NativeIdentityUi`, `SystemNativeIdentityUi`, `apply_native_identity_action` |
| `native::owner_settings` | App directory, data root, per-profile settings file |

Under `web` (portable, builds for `wasm32-unknown-unknown`):

| Module | Contents |
|---|---|
| `app` | `GraphshellApp`, `AppError`: local and remote projections through one `ClientState` |
| `mere_host` | `MereHost`, `SelectedPersonaRef`: Mere graph truth as a Graphshell endpoint |
| `product` | Graph-product operations and facets: `TransferScope`, `RelationFamilyFilter`, `EditableRelation` |
| `handlers` | `HandlerRegistry`, `HandlerOffer`, `OpenAddressV1` |
| `access` | `AccessRecord`, `AccessObservation`, `AccessTransition`, `save_access_record` |
| `capture` | `HistoryCapturePolicy`, `BrowserVisit`, `NormalizedVisit`, `CaptureBackend` |
| `browser_storage` | `StoragePersistence`, `decide`, `status_line` |
| `transfer` | `TransferRequest`, `TransferRouteV1`, `TransferAuthorization`, manifest and content facets |
| `transfer_endpoint` | `TransferSourceEndpoint`, `TransferBeginV1` |

Under `personal-sync`:

| Module | Contents |
|---|---|
| `personal_sync` | `PersonalGraphEvent`, `PersonalGraphRecord`, `PersonalGraphExt`, `PersonalEncryption` |
| `transfer_offer` | `TransferOfferV1`, `transfer_offer_rule`, `offer_address` |
| `native::personal_sync_host` | `PersonalSyncHost`, `PersonalSyncHostConfig` |
| `native::device_sync` | `personal_graph_id`, `SeedNote`, `BlobAction`, `resolve_data_root` |
| `native::pairing` | `pair_device`, `unpair_device`, `PairingFacts` |
| `native::graph_keys` | `GraphKeyGroup`, `OpenedKeyGroup`, `AbsorbReport` |
| `native::transfer_staging` | `offer_transfer`, `receive_transfer`, `released_blobs_for` |

## Features

| Feature | Scope | Key dependencies |
|---|---|---|
| `native` (default) | Admitted sessions, native transports, Personae composition, the binaries | `graphshell-endpoint` (`network`, `stdio`), `notochord`, `transport`, `personae`, `session-runtime`, `ssh-agent-lib`, `ssh-key`, `light-file-dialog`, Tokio |
| `web` | The portable graph and canvas cone | `mere` (`graph`, `canvas`), `chartulary`, `eidetic`, `muniment`, `url`, `sha2` |
| `personal-sync` | `native` + `web` + device synchronization | `muniment/redb`, `p2panda-core`, `p2panda-store`, `stickleback` |

Crates under `crates/` may not depend on this package.
`scripts/check_port_boundaries.py` enforces that, checks that `graphshell` has
exactly one manifest, and checks that the `web` cone's dependency tree excludes
Turnstone, Genet host packages, and Servo runtime packages.

## Binaries

| Binary | Required features |
|---|---|
| `g1_receipt` | `native` |
| `g4_sessions` | `native` |
| `g5_peer` | `native` |
| `n4_policy_receipt` | `native` |
| `h4_identity_receipt` | `native` |
| `h4d_browser_receipt_host` | `native` |
| `h4e_browser_import_receipt_host` | `native` |
| `h4f_agent_probe` | `native` |
| `graphshell_native_host` | `native` |
| `h6_transfer_peer` | `native`, `web` |
| `h7_sync_peer` | `personal-sync` |

## Browser surfaces

`web/Cargo.toml` is a separate package, `graphshell-web`, whose `[lib] path` is
`../src/web.rs`. It is a `cdylib` composing this port's `web` cone with Mere
Canvas, Cambium over Genet's DOM and layout seam, and NetRender over a WebGPU
canvas. Its browser dependencies stay out of the `graphshell` package.

### September 5 practice workspace proof

`web/practice.html` is a current-tree browser proof over one disclosed
Woodshed comparison. Relations, pitch-membership Compare, and History use
typed occurrence/comparison selection. Save/reopen validates source identity,
revision, occurrence IDs, and the complete comparison disclosure before it
restores the bounded workspace state.

The same WASM component runs in `web/practice-embed.html`; that page is an
embedded-wrapper proof, not the native Woodshed application. Relations use
real Seiche dragging, Grid/Scatter recipes, fixed-60 Hz physics with at most
three substeps, demand-driven redraw, and retained Cambium/Genet/Netrender
faces. Projection fields are not wired into this proof yet.

Build the web package and serve `web/` over loopback HTTP. Run the standalone
walkthrough at `practice.html?scenario=scenarios/practice_workspace.scn`, the
same-origin reopen at `practice.html?scenario=scenarios/practice_reopen.scn`,
and the wrapper check at
`practice-embed.html?scenario=scenarios/practice_embedded.scn`. The
current `docs/receipts/practice_workspace_receipt.json` carries the
current-tree receipt; it is not a release or final benchmark.

Its lib root is `#![cfg(target_arch = "wasm32")]`, so it compiles to nothing on
a native host and `cargo check --workspace` does not cover its code. Check it
for the target it is for:

```
cd ports/graphshell/web
cargo check --locked --offline --target wasm32-unknown-unknown
```

`scripts/cross-repo-smoke.ps1` runs this alongside the workspace gate.

[`web/extension`](web/extension) holds the Chromium and Firefox extension and
the `org.mere.graphshell` native-messaging registration.

The desktop resident is [`djinn`](../djinn), not a Graphshell binary. Djinn
owns persona-held stores and exposes these Graphshell session brokers; this
port stays independently usable for local session and admission composition.

## Where to look next

[`docs/`](docs) holds the dated receipt notes for each landed slice.
[`docs/receipts`](docs/receipts) holds the committed receipt artifacts and the
commands that regenerate them.

## Application appearance

The existing full web app (`index.html`, `co_op.html`) and the saved tree app
(`tree.html?app=local`) consume shared Tabard themes. Open **Appearance**, choose
a built-in or previously imported theme and a mode, or paste the JSON exported
from the shared Tabard workshop and choose **Import theme**. Import selects that
theme; its attached custom authoring modes appear alongside Light, Dark,
High Contrast Light and High Contrast Dark. This application is a consumer of
the workshop's existing theme definition, not a second theme editor.

The origin's `graphshell.application.appearance.v1` localStorage record contains
the selected shared `ThemeChoice` and imported shared definitions. Each choice
or import is persisted before it becomes active. Failed parsing or storage
leaves the valid preference active. Corrupt stored intake is reported and left
in storage; the application uses its built-in fallback until the user saves a
valid choice. Missing stored identities and modes retain the requested choice
and show Tabard's resolution diagnostic. The IndexedDB graph database,
projection definitions, remote session revisions and graph authority remain
owned by their existing systems.

Only roots explicitly marked `data-appearance-application` consume this origin
preference. The tree additionally requires its existing `app=local` product.
Bare generated/fixture/dataset trees and embedded roots consume the composing
host's inherited `--tabard-color-*` variables. They neither read nor write the
application preference or mount an appearance picker. The existing
`data-owns-title` marker continues to decide title ownership, including error
reporting. `embed.html` supplies a light role set to demonstrate this separation
after the same origin has saved a different standalone preference.

The optional Moot capsule panel follows these same composing-host roles. Its
review, grant, turn, keep and close lifecycle remain the existing applet host's
responsibility. Appearance refresh retains the applet stylesheet and changes
only the host's presentation mapping.

Derived modes use the shared explicit-mode Tabard/Tinct artifact. For an
attached authored mode sheet, Graphshell supplies the shared default built-in
roles for omitted properties and then appends the **exact authored CSS rules**
after its application role mapping in the existing Genet stylesheet cascade.
It does not claim to derive arbitrary CSS from the imported theme's seeds.

Canvas and the browser semantic overlay use browser-computed shared colors.
An invisible same-origin document evaluates the sheet without injecting its
selectors into the containing page. Canvas backdrop and edges consume
`--gs-canvas-background` and `--gs-canvas-edge`, defaulting to
`--tabard-color-bg` and `--tabard-color-text-dim`. Retained derived faces use
primary, secondary, tertiary, success, danger and text through Pictograph's
shared `DerivedFacePalette::from_palette` mapping, including its existing
companion-color blend. Graph node selection/focus/status remains semantic
canvas state. The local executable projection preview shares the same sheet;
remote presentation continues to own its authored scene.

For CSS that needs to affect Canvas or semantic DOM paint, set the shared
properties at `:root` in the attached sheet, or inherit them onto the embedded
component. Arbitrary selector rules such as `.topbar { border-width:4px; }`
remain authoritative for Genet chrome; they do not redefine Canvas colors.
The role bridge is the documented seam, with explicit shared defaults for
omitted roles. It is not a separate renderer or CSS-to-seed conversion.

### Appearance qualification commands

Run standalone commands from `ports/graphshell/web` so its committed getrandom
WASM configuration applies. Its generated lock is independent of Mere's root
lock; create it once before the locked gates if this checkout has none.

```sh
cargo generate-lockfile --offline
cargo check --locked --offline --target wasm32-unknown-unknown -j2
cargo check --locked --offline --target wasm32-unknown-unknown --no-default-features -j2
cargo check --locked --offline --target wasm32-unknown-unknown --features applets -j2
cargo build --locked --offline --target wasm32-unknown-unknown -j2
wasm-bindgen target/wasm32-unknown-unknown/debug/graphshell_web.wasm --target web --out-dir pkg
python3 -m http.server 8872
```

Run the portable adapter regression suite from the Mere root:

```sh
cargo test --lib --locked --offline -p graphshell --no-default-features --features web appearance::tests
```

Use one browser origin/profile for these full application scenarios, in order:

1. `http://127.0.0.1:8872/index.html?scenario=scenarios/appearance_modes.scn`
2. `http://127.0.0.1:8872/index.html?scenario=scenarios/appearance_reopen.scn`
3. `http://127.0.0.1:8872/embed.html?scenario=scenarios/appearance_embedded.scn`

The first drives the actual semantic controls, captures four canonical modes,
imports the attached CSS fixture, selects its custom authoring mode and rejects
corrupt JSON without losing the valid selection. The second proves a fresh
page reopens the same imported mode. The third proves the embedded role set
and containing page title/controls survive the saved app preference. Inspect
`window.graphshellScenario()` for completion, page/GPU failures, captures and
snapshot evidence. `graph-nodes` is included in the full app snapshot for
before/after graph authority checks.

Also open `tree.html?app=local` in that profile: it must reopen the same custom
mode, switch canonical modes through the same browser picker, update its
Genet overlay and Canvas, and reopen again. Bare `tree.html` and `dataset.html`
must expose `data-appearance-owner="host"` and no appearance picker. Review
actual browser captures and GPU/error gates; successful WASM checks alone do
not establish visual or reload acceptance.
