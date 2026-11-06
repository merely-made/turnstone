# Graphshell browser extension

The Chromium and Firefox extension that carries Graphshell's Wasm portal, plus
the `org.mere.graphshell` native-messaging registration that connects it to the
resident device host.

## Files

| File | Contents |
|---|---|
| `manifest.chromium.json` | MV3 manifest: `background.js` service worker, fixed public `key` |
| `manifest.firefox.json` | MV3 manifest: `capture-model.js` + `background.js` background scripts, add-on id |
| `background.js` | Browser action opens `graph.html`; the local profile record; history and permission listeners; the bounded `storage.local` capture queue; refuses messages from anything but its own pages |
| `graph.html`, `graph.css` | The extension workspace: the shared Graphshell component, the browser-history consent panel and the second-view notice |
| `capture-model.js` | `GraphshellCaptureModel`: default policy, visit sanitization, queue bounds, the ledger's conflict rule |
| `../extension-keep.js` | The **Page text** panel of `graph.html`: the optional access request and refusal, the size limit, kept/failed/omitted counts and browser storage state; hands queued text to the app and acknowledges it on the app's word |
| `follower-queue.js` | `GraphshellFollowerQueue`: a following view's edit transport on the views channel (send, acknowledge, resend once to a new writer, give up), and the writer's side (in-flight dedupe, no handover mid-edit); `smoke-writer-handover.mjs` runs it |
| `action-form.js` | `GraphshellActionForm`: builds an intent payload from an advertised `input_form` |
| `bridge.html`, `bridge.css`, `bridge.js`, `resource-chunk.js` | The native surface: connects `org.mere.graphshell`, pulls bounded resource chunks, renders identity cards and advertised actions |
| `qualify-keep-content.py` | Headed Chrome qualification of kept page text on a fixture profile: a local test server, the control and its permission, loaded pages, a goal that only page bodies match, and a cold reopen |
| `qualify-goal-surface.py` | Headed Chrome qualification of the goal surface and durable tag suggestions on a fixture profile, with a cold reopen and a follower view (reuses the hidden-writer harness) |
| `prepare-extension.ps1`, `prepare-extension.sh` | Assemble one unpacked extension directory |
| `install-native-host.ps1`, `install-native-host.sh` | Register the native host for the current user |
| `native-host.chromium.json.in`, `native-host.firefox.json.in` | Native-host manifest templates; `__GRAPHSHELL_NATIVE_HOST__` is replaced with the binary path |
| `fixtures/capture-conflicts.json` | Visit pairs with the encounter ledger's verdicts, written by `tests/capture_conflict_fixture.rs` |
| `smoke-action-form.mjs`, `smoke-capture-model.mjs`, `smoke-capture-background.mjs`, `smoke-capture-ack.mjs`, `smoke-native-host.mjs`, `smoke-transfer-accept.mjs`, `smoke-resource-chunk.mjs`, `smoke-keep-text.mjs` | Node smoke tests |

`prepare-extension` also copies `graph.html`, `graph.css`, `styles.css`,
`loader.js`, `mount.js`, `extension-profile.js`, `extension-keep.js`, the fonts and their notices
(`IBMPlexSans-Regular.ttf`, `IBMPlexMono-Regular.ttf`, `Newsreader-Regular.ttf`,
`NotoSansMath-Regular.ttf`, `OFL-IBMPlex.txt`, `OFL-Newsreader.txt`, `OFL-NotoSansMath.txt`), the segmenter dictionaries and their
notice (`dictionaries/`, fetched only when reading text needs them), and
`pkg/` from the parent `web/` directory into the destination. `dist/` is
gitignored, so it is the usual destination.

## Local profile

The installed extension is useful without the native host. `background.js`
mints one `graphshell.local-profile/v1` record (`id`, `browser`) in its own
storage; the page passes it to the host, which opens the
`graphshell-local-v1` IndexedDB store with `GraphshellApp::open_local`: the
latest session, or an empty one. Nothing is seeded. Local work and captured
visits are attributed to `graphshell://local-scope/<id>`, not to a Personae
persona, and that scope claims no custody: signing, grants and native
dispatch name the provider they need. `?fixture=` routes keep their own
stores and fixture identity.

One view writes the store at a time, and any other view follows it. A page
takes the `graphshell.local-profile.writer.v1` Web Lock for its lifetime
before reading the queue. A second tab or the side panel then mounts as a
follower (`graphshellViewRole = "follower"`):
- It opens the committed session with `GraphshellApp::observe_local`, which
  writes nothing (no epoch advance, no migration).
- pandect refuses every edit in that host before it runs
  (`SessionError::Observing`).
- It hands no store to documents, panes or retained content.
- It takes no capture.

The writer posts `committed` on the views channel whenever `data-writer-busy`
returns to false. Followers read the store again then, and whenever they are
shown again.

A follower offers nothing that stores a change. Its details are text, and
its command menu lists storing commands disabled with the reason. Its bar,
at the bottom of the page, offers **Show that view** and **Edit here**. The writer
hands over by reloading, only when nothing is being saved or awaiting retry,
and comes back as a follower. The view taking over keeps the lock and remounts
its element as the writer. If the writing view closes, a follower takes the
lock within about 2 s. A follower hidden in a background tab catches up when
shown, because hidden tabs run no frames. `graph.html` also opens as the Chrome
side panel and the Firefox sidebar.

Selected objects offer **Copy link**, which writes the exact stored address
(query and fragment kept) to the clipboard in the click and reports refusal
rather than success, and **Open in this browser**, the `browser.attached`
handler: an accepted HTTP(S) open becomes `tabs.create` in this browser. Graph
ids, inline data and browser-internal schemes are not copied as links; other
schemes are not opened.

## Identities

| Browser | Extension id |
|---|---|
| Chromium | `oajkkocppbpbmfblepgbiidagliniofd`, derived from the manifest `key` |
| Firefox | `graphshell@mere.systems` |

The same values are `djinn::door::browser_carrier::CHROMIUM_EXTENSION_ID` and
`FIREFOX_EXTENSION_ID`. Set `GRAPHSHELL_EXTENSION_IDS` to a comma-separated
list to add locally built ids; the native host matches each id exactly. Store
release ids can replace the defaults through the installer.

## Permissions

`nativeMessaging`, `storage`, `unlimitedStorage` and (Chromium) `sidePanel` are
installed permissions. `unlimitedStorage` keeps the local profile's store without
a persistence prompt. In Firefox it also makes `navigator.storage.persisted()`
true; Chromium still reports false. Elsewhere the mount waits at most 1.5 s for
`persist()`, because Firefox holds that promise until a person answers its
prompt; an unanswered request is reported as unknown. `history`, `tabs`,
and `webNavigation` are optional; the portal's **Enable capture** control
requests `history`. The default policy in `capture-model.js` has `enabled:
false`. Visits are sanitized against the query-string and origin-exclusion
settings before they enter the `storage.local` queue. Consent and those
settings live in the **Browser history** panel of `graph.html`.

Every write to the policy, queue, visit map and observation floor runs in one
serialized order, so a configure or a permission removal cannot overwrite a
visit queued meanwhile. The queue keeps the first visit for each source
identity. A later visit whose source facts differ (address, time, transition,
parent, original microseconds or locality; a title may differ) is refused alone
by the encounter ledger's own rule and counted in `coverage.refused_conflicts`,
never failing the batch. `tests/capture_conflict_fixture.rs` judges visit pairs
with the ledger and holds `fixtures/capture-conflicts.json` equal to its
verdicts; `smoke-capture-model.mjs` replays them through `mergeQueue`.

The Wasm host commits one Muniment batch (the graph document, the `LocalOnly`
AccessRecords, the Eidetic browsing traces and the encounter ledger). Only then
does it write the accepted, dropped and buffered counts to `data-capture-*` on
the Graphshell root and dispatch `graphshell-capture-complete` on the document.
The extension page acknowledges the visits it pulled when that event arrives and
the queue drains. A failed batch dispatches nothing, so its visits stay queued;
redelivery is absorbed as duplicates by the ledger.

Visits also reach a view that stays open. When live visits are admitted,
`background.js` sends `capture.available`. With no view open, the browser
rejects that send and the visits wait for the next open. The writer view then
pulls the queue and hands it to the host as one numbered
`graphshell-capture-batch` event. Only one batch is in flight at a time, and
notices that arrive meanwhile cause one more pull afterwards. The host applies
the batch when no other save holds the graph, and the batch blocks edits and
handover until it lands. It then writes `data-capture-batch` (the number),
`data-capture-batch-error` and the batch's counts, and dispatches
`graphshell-capture-batch-complete`. The page acknowledges only a batch that
committed without error. A failed or malformed batch stays queued for the next
notice or open. A standby view takes no visits. Firefox can refuse a message
that reaches its event page while it suspends ("Receiving end does not exist");
the page retries that error three times and no other.

Capture attribution comes from the `SelectedPersonaRef` and device reference
injected by the composing host, not from the selected vault profile or a
device-roster entry. In the installed extension that is the local profile's
scope and browser.

## Encounter intake

The extension persists a profile-instance and history-generation marker in its
own storage. `capture.status` exposes that source scope for deliberate binding
to a file export; the marker is independent of a Personae vault or persona.
Reconciliation queues every available visit since live remembering was enabled,
retaining referring visit IDs. Worker restart retains the scope and observation
floor. Imports filter individual visits by the selected range; they report
candidate saturation, queue omissions and unknown source completeness.

Eidetic's encounter ledger uses Chartulary/Stemma explicit-parent admission.
Children awaiting missing or outside-range parents remain durable intake, and
the portal reports them as awaiting parents. Resolved visits project actual
document traversals and `BrowserDocumentRelation` evidence with exact source
handles. `MereHost::browser_relations` reads that evidence. The receiving graph's
ledger, access records, traces and graph changes commit in one Muniment batch;
live navigation cursors are not replayed through imported history.

`BrowserVisit::from_imported` joins file exports to the same admission path.
The Firefox exporter requires explicit `--profile-id` and
`--history-generation`, and retains `from_visit` identity and microseconds.
Bind these values deliberately to an extension's source scope when both describe
one history source; a new file/run hash is not a profile identity.

The 2026-10-12 slice has native automated and fresh-process Redb receipts,
Chromium/Firefox API simulations and browser-target compilation. Its native
fixture is synthetic and unsealed. Renewed headed browser/IndexedDB behavior,
tab/document observation, automatic source-generation replacement handling,
whole-input evidence retention, trace source handles, multi-writer admission and
private custody remain open. Forget removes browser encounter/relationship
evidence; the shared resource registry has independent identity ownership.

## Admission

The extension and `graphshell_native_host` hold presentation and connection
state. The selected Personae profile, vault, SSH keys, delegation issuer, and
`SessionHello` signer stay in the Djinn resident.

Installing the native-host manifest pairs one exact extension id with the relay
binary. The relay connects to the per-user device broker, and each connection:

1. receives a fresh native-host challenge;
2. answers with that challenge and a fresh browser nonce;
3. derives a link id from the browser launcher id and both nonces;
4. runs signed `SessionHello` admission over that link;
5. releases the Graphshell request stream after admission accepts.

Native messaging frames are capped at `MAX_NATIVE_MESSAGE_BYTES` (1 MiB). On
Windows the device broker compares the connecting client's process token SID
with its own before reading the broker hello.

SSH-key import is a native-only interaction after admission. The extension
sends the projection session id and selected unlock policy; the picker path,
key bytes, and passphrase stay in the native host, and the extension receives a
public import receipt or a bounded failure reason.

`bridge.html` is the admitted native surface and is linked from the graph
portal as **Open Identity**. An endpoint may advertise a bounded `input_form`;
the bridge renders its labels and choices and serializes the declared schema
with the selected values.

## Building

Build the relay and the Djinn resident (the relay is Djinn's since the stack
lift plan's P5; its binary name is unchanged):

```text
cargo build -p djinn --bin graphshell_native_host
cargo build -p djinn --bin djinn
```

Start `djinn` under the platform's resident lifecycle before
opening the extension. On Windows,
[`../../mere-resident/install-windows.ps1`](../../mere-resident/install-windows.ps1)
performs the reversible standard-agent cutover; it requires the expected live
fingerprint and preserves the previous Personae task.

Prepare an unpacked extension directory:

```text
wasm-bindgen /path/to/graphshell_web.wasm --target web \
  --out-dir ../pkg --out-name graphshell_web
./prepare-extension.ps1 -Browser chromium -Destination ./dist/chromium
./prepare-extension.ps1 -Browser firefox -Destination ./dist/firefox
```

Register the built native host for the current user:

```text
./install-native-host.ps1 -BinaryPath /absolute/path/to/graphshell_native_host.exe
```

## Keep page text

An optional control, off by default and separate from remembering visits (the
**Page text** panel; `keep.*` messages beside `capture.*`). Turning it on asks
the browser for the optional `scripting` permission and optional access to all
sites (`optional_host_permissions: <all_urls>`); a refusal leaves it off and
says so, and removing either permission later turns it off and discards queued
text. There are no content scripts and no background fetches: when a tab
finishes loading, `background.js` runs `GraphshellCaptureModel.extractPageText`
in that page with `scripting.executeScript`. It reads the visible text of
`main`/`article` (else the body), skips scripts, styles, navigation, forms,
hidden and embedded content, and returns nothing for a document that is not
HTML. Private windows, an unaccepted scheme and the origins excluded under
**Browser history** are not read. Text is bounded in UTF-8 bytes (32, 128 or
256 KiB per page; `max_bytes`), queued in `storage.local` beside the visit
queue (one pending text per address, 48 pages, 6 MiB; overflow is counted),
and delivered a few pages at a time. The worker never opens IndexedDB. The
writer view passes a delivery to the app, which records each page with
`eidetic::PageTextStore` (`kept_text.rs`); only after it reports the counts is
the delivery acknowledged. A store failure (quota included) is reported in the
panel and the pages stay queued. Forgetting an address also forgets its kept
text. The panel shows kept, unchanged, waiting, failed, omitted, cut-to-size
and skipped counts, the last problem, and the browser's storage use,
quota and persistence state.
