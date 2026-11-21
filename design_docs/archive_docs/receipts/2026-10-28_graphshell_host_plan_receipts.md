# Graphshell integration receipts

Run evidence for [the Graphshell integration plan](../../2026-10-28_graphshell_host_plan.md).

## 2026-11-14 N1 native resident graph on Windows

The local Turnstone checkout was based on `40a9c19da3e723ad43ace116f0c77bf7be4420b7` with the native pane and concurrent work in the tree. The local Graphshell checkout for Djinn and the fixture examples was based on `f9b4744214719da3259a2c7b9406a98732e9b2a4`, also with local changes. Both builds used Rust 1.99.1 and their lockfiles offline. Binary hashes are recorded in each run's `inputs.json`.

`Code/testing/turnstone/receipts/2026-11-14_resident_graph/run.py` starts a fixture custodian and a separate Djinn on private named pipes, seeds two sessions under a granted domain and another graph under an ungranted domain, and drives the normal Turnstone window. The installed resident and user profile are outside the run. The standalone graph window is absent from the driver.

`acceptance/open` opened the granted graph via the command palette, inspected one record in each session, and selected Grid. `acceptance/reopen` restarted Turnstone against the same workspace and resident. It restored the pane and the most recent session's selected record. Switching to the older session recovered its selected record without a fresh click. Both scenarios saved nonblank captures. `acceptance/ungranted` reached the broker refusal and exposed an end-of-stream message. The broker log identifies the refused route. That run failed its message assertion and prompted the grant guidance in the pane. The earlier `unavailable` scenario showed Reconnect with no resident on its configured endpoint.

The locked Turnstone library tests passed on the Windows checkout (1,403 passed, 0 failed, 9 ignored). The 600-line source check and Graphshell app and web dependency boundary check passed.

The rerun is in `Code/testing/turnstone/receipts/2026-11-14_resident_graph/windows-n1/`. In `open` and `reopen`, the app again opened and inspected both sessions, then restored the pane and saved focus after restart. Both toolbar captures show Grid selected for the older session before and after restart. `ungranted` verified the route-grant guidance and Reconnect in the window. The broker log confirms refusal for `mere/different-resident-graph`.
The `unavailable` scenario used a named pipe with no resident and verified the unavailable message and Reconnect. All four scenarios returned `RESULT ok`. The driver stopped the fixture custodian and Djinn and the generated stores were removed after the run.


## 2026-11-18 N1 native resident graph on T-PC

The resident pane was extracted from the Windows working tree and applied to
Turnstone `main` on T-PC. The first Linux check ran with Turnstone based on
`f6790128` and its Graphshell dependency at `4c52c05`. Djinn and its two
fixture helpers were built from Graphshell `0e66bbc`. The fixture created
a separate profile, vault, and Unix sockets.

The headed native window passed all four scenarios. The opened pane
inspected each seeded session. After restart, it reopened the most recent
session and restored the older session's focus and Grid layout when switched
back. The denied case showed the grant guidance and Reconnect. The Djinn
log records the refusal of `mere/different-resident-graph`. The unavailable
case showed Reconnect against a socket without a broker.

The captures and binary hashes are in
`~/Code/testing/turnstone/receipts/2026-11-18_n1/linux-native` on T-PC.
The Linux driver reuses the isolated resident helper in
`support/graphshell-coop/resident.py` and the Windows N1 scenario steps.
The verified Grid captures are `open/inspected-and-switched.png` and
`reopen/reopened-saved-view.png`. The fixture stores were removed after
capture and the scenario output and logs were retained.

The full library suite exposed two test assertions tied to score version 5.
The pinned format is version 6. The assertions now check the current score
version and still require a refusal to preserve the active snapshot and
resource state. The first scoped Linux suite passed 1,431 tests with nine
ignored after that correction.


The final rerun used Turnstone base `c47ba17a` and Graphshell `9f3537a`
from the uxtree P7 resolved lock. All four headed cases passed again, with
six nonblank captures under `~/Code/testing/turnstone/receipts/2026-11-18_n1/p7`.
The pinned Turnstone library suite passed 1,432 tests with nine ignored.
The fixture daemon and helpers were the same binaries as in the first run.

## 2026-11-18 N2 resident kept pages on T-PC

The native Turnstone candidate used Graphshell `066ecd0` and the resident's
sealed-wallet test fixture. The fixture served the real application broker on a
private Unix socket and granted Turnstone a pair of fixture meres. The app
ran with isolated profiles on the Wayland desktop. The captures, scenario
logs, fixture source, driver and binary hashes are in
`~/Code/testing/turnstone/receipts/2026-11-18_kept` on T-PC.

- `final/capture` opened **Kept pages**, enabled keeping, captured a local
  fixture URL, found it by body text and opened the result in the pane.
  Returning from the reader preserved the search results. Searching for a
  marker in a script element yielded zero matches.
- `final/reopen` restarted Turnstone with the same profile and broker. It
  found and opened the kept text again from the restored resident pane.
- `final/other-mere` ran the same search under another admitted mere and
  found zero matches. The loopback server recorded a single GET across
  the three runs, for the explicit capture. Each run saved a nonblank
  screenshot and returned `RESULT ok`.
- The initial `first` run used Djinn without a configured retained-text
  wallet. It showed the custody pause and recorded zero requests. That
  run stopped at the positive capture assertion. The successful capture
  receipt uses fixture wallet custody.

The locked native Turnstone build and library tests passed (1,432 passed
and nine ignored). The standalone viewer's five kept-page tests and ten
resident retained-text tests passed. The resident tests include toggling
keeping while preserving the owner's excluded origin and page-size limit.

The resident's ten tests passed again with Graphshell's concurrent uxtree
changes merged at `4af0249`. The N1 regression sweep in
`~/Code/testing/turnstone/receipts/2026-11-18_n1/kp` passed all four native
cases with the kept-pages toolbar present.

Turnstone's merge with the resource-neighborhood work at `0f17e4db` kept the
qualified Graphshell dependency at `066ecd0`. Its native build and all
three headed kept-page scenarios passed again under `merge/`. The merged
library suite passed 1,433 tests with nine ignored. The loopback
server again recorded only the explicit capture. The merged reader capture
shows the body in the pane at 1024 × 600. Generated fixture stores and the
temporary broker executable were removed. The retained receipt fits within
the workspace's 100 MB cap.

## 2026-11-18 N2 advertised record actions on T-PC

The native headed run used an isolated Djinn with its web address handler
enabled and an owner grant for one fixture mere. The driver seeded a loopback
page through the ordinary application route and selected its record in
Turnstone. It recorded the exact request sent by Turnstone and the reply
from Djinn. The scenario files, screenshots, binary hashes, and bounded
wire transcript are in `~/Code/testing/turnstone/receipts/2026-11-18_actions`
on T-PC.

- `native/accepted` clicked **Open in browser** in **Record actions**. Djinn
  accepted the command and the loopback server recorded one navigation to
  `/approved` plus its favicon request. Turnstone showed owner acceptance and
  the approved URL as its current address.
- `native/stale` advanced the real graph through a second admitted client
  after Turnstone submitted its request. Djinn reported revision 7 for the
  request observed at revision 6. Turnstone asked the user to reopen the
  action. The loopback server recorded zero requests.
- `native/refused` preserved Turnstone's original bound request in the
  transcript, then substituted a mismatched address in the fixture relay to
  exercise the real owner's rejection. Turnstone displayed the owner's
  reason. The loopback server recorded zero requests.

All three native scenarios returned `RESULT ok` and saved nonblank captures.
The native build and the locked library suite passed, with 1,434 passed and
nine ignored. The focused bounded-form test verifies the exact selected
value and that a form ceases to match its offer after a revision change.
The retained receipt fits within the workspace's 100 MB cap.

## 2026-11-18 N2 Personae on T-PC

Turnstone mounted the standalone viewer's shared Dramatis pane and worker
inside a contributed surface. It used Graphshell's `dramatis-review-host`
fixture built at `4af0249`. The fixture serves the app broker with the
Dramatis custody grant for two synthetic personae and quarantined imports.
Unlock uses the fixture's Djinn prompt provider. The Linux scenarios, native
captures, hashes, and logs are in
`~/Code/testing/turnstone/receipts/2026-11-18_personae` on T-PC.

- `p4/connected` opened **Personae** from the command palette and read the
  vault and quarantined import metadata. Tab traversal brought **Choose Work**
  into the viewport and Enter selected it. The run returned to Family, locked
  the vault, and unlocked it through Djinn. The import cards returned after
  unlock and the passphrase action was offered again.
- `p4/unavailable` opened the pane against an absent app socket, reached
  **Reconnect**, and presented identity as pending after retrying. Both
  scenarios saved nonblank captures and returned `RESULT ok`.

The shared Dramatis pane's twelve tests exercised the bounded import form,
exact review targets, keyboard focus, and pending and cancelled prompts.
The Personae pane connects under the Dramatis declaration. On Windows a grant
pinned to the standalone viewer's image requires owner approval for
Turnstone. The headed run exercised the isolated Linux custody and prompt
fixture. H4r tracks host-specific native prompts and carry management.

The final Turnstone library run passed 1,437 tests with nine ignored.
The fixture stores and generated fixture executable were removed after the
run. The retained evidence fits within the workspace's 100 MB cap.

## 2026-11-19 N2 saved view-state handoff on T-PC

The headed run used Turnstone `890d0eba` with its current dependency lock
and the candidate Djinn based on Graphshell `2b0b5ff4`. The exact Graphshell
source delta and executable hashes are recorded in `inputs.json`. All
fixture changes and inspections used the admitted application routes.
The scenarios, screenshots, and exact before-and-after reads are in
`~/Code/testing/turnstone/receipts/2026-11-19_view-handoff/linux-headed`
on T-PC.

- With the owner setting off, the native pane opened the legacy-only
  session in Spectral with no selected record. Both Turnstone slots for
  that session were still absent in the admitted read.
- With the setting on, first open recovered Graphshell's Grid layout and
  the saved selected record in that session. The other session showed
  Turnstone's existing Spiral layout and its own selected record. Both
  original Graphshell views matched their readings from before the handoff.
- After restarting Turnstone and Djinn with the setting off again, both
  Turnstone views matched their first-open state. The restarted window
  showed both expected layouts and focus. All three scenarios returned
  `RESULT ok` with five nonblank captures.

Djinn copies the layout strategy for the session and focus from its
device-local attention slot. Older journaled views can supply layout.
Writable live sessions participate, and the archive test confirms the
archived session stays read-only. The layout entry, device attention
update, and handoff markers share a transaction. Graphshell's seven
attention tests passed, including preservation of existing and opaque
Turnstone view records and a competing write that refused the whole
handoff. Turnstone's locked library suite passed 1,420 tests with nine
ignored. The generated profile and custody stores were removed.

## 2026-11-20 N2 native co-op on T-PC and S-PC

The native receipts use Turnstone `243ec745` and Graphshell `cfb155fb`.
The frozen executable hashes and resolved lock hashes are in each packet.
Both desktops used the same four frozen Linux x86-64 binaries. T-PC ran
Fedora 45 with AMD Vulkan. S-PC ran Ubuntu 26.04.1 with AMD Vulkan and a
process-scoped glibc 2.44 loader and runtime. The guest graphics receipt
records the loaded Vulkan driver, hardware render node, and exact runtime
file hashes. The active graph carrier was direct Iroh over IPv6.

The two-device packet is `~/Code/testing/c2e/receipt` on T-PC. It passed
`verify.py` with seven native Turnstone processes, four Djinn starts,
two fixture custodians, and 22 reviewed nonblank captures.

- The owner attached the shared graph, opened **Co-op**, and issued an
  editor invitation to a contact exported by Turnstone on the other
  desktop. The guest opened **Invited graph**, added a record, and renamed
  an existing source. The accepted edit remained attributed to the guest
  in the owner’s durable graph.
- While the owner viewed the private graph, the guest continued to see only
  the shared graph. The guest received a regular owner-side graph edit on
  the same connection. Before and after readings agreed for the private
  graph.
- Ending guest access cleared the guest graph and editor controls. After
  restarting both residents and viewers, the same invitation was rejected.
  The owner reopened the shared graph, retained the prior edits, and saved
  a further edit. The owner graph scrolls above the viewport when using the
  invitation controls. Live withdrawal displays a connection-loss message.
  Cold reopen explicitly reports `Delegation(Revoked)`.

The stopped-resident control packet is `~/Code/testing/c2b/receipt`. It
passed `verify.py` with three reviewed nonblank captures. The native pane
showed **Reconnect** and connection refusal while the resident was stopped,
and its wait for the shared graph expired with no session attached.

Each Djinn used an external synthetic fixture custodian. Consent was
represented by the narrow fixture grants. Installed custody prompts,
relay and recovery, and standalone packaging retirement need separate
checks. The Y-PC preflight failed to obtain a Vulkan adapter and is
retained as a hardware finding. The direct and baseline packets together
fit within the workspace receipt cap. Turnstone's locked library suite
passed 1,429 tests with nine ignored. The harness passed all 13 Python tests.

## 2026-11-20 N2 standalone viewer retirement

Cargo no longer resolves the `graphshell-mere` workspace package or
lists `graphshell_mere` as a binary. The native staging directory held
one product executable, `turnstone`. Djinn's native-messaging relay
and the developer fixtures remain separate targets. The resolved
Turnstone package set drops only `graphshell-mere`, and Graphshell
remains pinned to `d50bc125`.

The T-PC headed check ran the staged Turnstone against the admitted
sealed-wallet fixture. The exact executable, fixture sources, locks,
previous source commit and patch hash are in
`~/Code/testing/c-retire/resource-links/inputs.json`. The four reviewed
captures show these steps.

- The person kept two loopback pages and opened the retained target text
  through the existing search.
- **Follow kept links** started from the first page. Depth zero recorded
  one visit and no matches. Depth one followed its graph link and cited
  the target's retained version, highlighted terms, coverage and the
  reason for each visit. HTTP logs contain only the two requested
  capture GETs.
- The same search in a second mere returned no match.

The first production-shaped test profile had no retained-page wallet.
The sealed fixture supplied it without any personal data. The headed
run also exposed a nested scrolling issue, which was fixed by letting
the resident pane own scrolling. The final locked library suite
passed 1,431 tests with nine ignored, including two focused tests for
the shared snapshot and invalidation on pause, error or attachment change.

## 2026-11-20 H5r extension native messaging after retirement

T-PC Firefox 160 loaded the Firefox extension once from the
pre-retirement Turnstone `205bdded` sources. The disposable profile,
add-on origin and ID, native host manifest, and relay pathname were
the same on both sides of the broker restart. Graphshell `d50bc125`
supplied `graphshell_native_host` and production-shaped Djinn. The
relay ELF stayed byte-identical. Djinn restarted with the same
external synthetic custodian.

Both native sessions showed three public cards and the same public
subject and profile fingerprint. The session IDs and Djinn processes
were distinct. Firefox sent Close for both, and Djinn recorded
`end=Closed`. With Djinn stopped, the same bridge displayed
**Native host disconnected** with no cards and the Close control
disabled. The Firefox and Chromium native-message framed checks also
passed. The headed packet and exact hashes are under
`~/Code/testing/c-h5r/run1/receipt` on T-PC.

The POSIX preparer now includes `action-form.js`. The PowerShell
preparer copies `extension-keep.js` from the web root. Linux and
Windows produced the same 30 files by SHA-256 for each browser, with
the existing manifest identities. The package inputs used an earlier
provenance-checked default Wasm bundle. The headed check exercised
the native bridge, which is static JavaScript. A fresh Wasm runtime
qualification and signed add-on persistence across a browser restart
remain separate checks.

## 2026-11-20 H5r fresh Wasm and browser restart checks

T-PC built the default `graphshell-web` product from Turnstone
`c8b749be` plus the context-scene rule fix in this commit. Context rules
take text, numbers and booleans as facts, matching the native presenter.
Reference, date, date-time and list values remain available in the source row.

The unmodified dependency build failed on Wasm in `cubecl-runtime
0.12.0-pre.2`. The test used a disposable copy of that exact crate with
[CubeCL's upstream fix](https://github.com/tracel-ai/cubecl/pull/2050)
applied via a Cargo path override. Both resolved lockfiles, the original
errors and the patch are in the receipt. This qualification used all
default features and wgpu 30.0.2. The checkout still needs a released
CubeCL fix for an unmodified default build.

Wasm-bindgen 0.2.129 generated the web package. A section comparison
verified that removing the debugging name section preserved every other
section byte-for-byte. The served Wasm was 83,512,762 bytes with SHA-256
`49a58e03d0303208030c57b53617c58a8da41a9bf03007b2371cce5f51cd6bc5`.

Headed Firefox 160 on Wayland loaded the fresh package. The repository
dataset rendered 21 nodes and 30 relationships and exposed all 30
relationship names. The stale-revision control exposed an accessible
refusal and rendered zero nodes and edges. Both scenario and window
captures are retained.

The original projection scenario failed at Add an object and Address
because it never opened Document tools. The rerun added the two pane
disclosure clicks and kept every assertion. Save, undo, redo, and
fresh-page reload all passed on the same origin and profile. Firefox
also logged script timeout and Wasm `unreachable` errors during the
headed tests that the scenario's page gate did not report. A clean
Firefox runtime log remains open.

Headed Chrome for Testing 151.0.7922.34 on Wayland loaded the exact
same Wasm bytes from the Chromium extension. The native bridge admitted
three public cards using the existing Graphshell `d50bc125` relay and
synthetic Djinn fixture. The stopped-resident control showed a refusal
with zero cards and Close disabled. Fresh sessions after Djinn and
Chromium restarts read the same subject and fingerprint, and accepted
Close.

The packaged graph saved one local record and disclosed it once on
reopen in a new Chromium process. The unpacked CDP install needed to
load again on process restart. The new process first reproduced an
unavailable extension origin, then loaded the byte-identical package
into the same profile. The graph record survived in that profile.
The graph used WebGL2. The inspected graph captures show overlapping
history and page-text controls and a clipped details pane at 1366×593.

Headed Firefox 160 permanently installed a Mozilla-signed Web Platform
Tests add-on in a disposable profile. Firefox refused the same-ID
unsigned package with `ERROR_SIGNEDSTATE_REQUIRED`. Both Firefox
process groups exited cleanly. The second process used the same profile,
where the add-on was still active at version 1.0 with signed state 2
and the same XPI hash. The driver made one signed install call and
removed the profile after verification.

A signed Graphshell XPI is still needed for the product restart check.
The configured Linux/Windows hosts and both reachable Macs supplied no
matching signed artifact. The signed fixture exercised Firefox's
permanent install path.

The T-PC packet is
`~/Code/testing/turnstone/receipts/2026-11-20_fresh-extension-runtime`.
`bundle-provenance.json` records the source diff hash, Cargo resolution
and package bytes. `wasm-runtime` and `wasm-runtime-projection-disclosed`
hold the Firefox Wasm attempts. `chromium-tpc-4/receipt/chromium.json`
records the Chromium run, and `signed-fixture-restart.json` records the
Firefox signing control and both process exits.

## 2026-11-20 H5r Turnstone Companion display name

Firefox 160 and Chrome for Testing 151 opened the branded workspace and
Identity page in headed disposable profiles. The workspace kept its
Turnstone Companion title after the Wasm mount. Both capture controls
started off. Firefox reported `graphshell@mere.systems` and Chromium
reported `oajkkocppbpbmfblepgbiidagliniofd`. The relay registration still
names `org.mere.graphshell`, and the extension manifests changed only
display text. Eight JavaScript smoke checks passed with Web Storage
enabled in Node 24.

Visual review confirmed the branded workspace header in both browsers.
The existing Browser history and Page text controls still overlap.
Both packages used the fresh default Wasm build, with the same
disposable CubeCL override as the preceding H5r build. The unsigned
Firefox ZIP has SHA-256
`98d0bda687e508ff193bb1396ec992389123125f43c3115cb9c0c8b499a61855`.
Mozilla's `web-ext` 10.8.0 lint returned zero errors and four warnings.
One flags the missing data consent declaration. Two flag dynamic
imports in the shared web loader and viewer mount. One flags the
generated wasm-bindgen `innerHTML` setter. The package and receipts are
in `~/Code/testing/turnstone/receipts/2026-11-20_companion-signing` on T-PC.
