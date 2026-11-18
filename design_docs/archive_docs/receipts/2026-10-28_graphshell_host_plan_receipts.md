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
