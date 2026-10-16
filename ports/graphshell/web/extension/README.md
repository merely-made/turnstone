# Graphshell browser extension

The Chromium and Firefox extension that carries Graphshell's Wasm portal, plus
the `org.mere.graphshell` native-messaging registration that connects it to the
resident device host.

## Files

| File | Contents |
|---|---|
| `manifest.chromium.json` | MV3 manifest: `background.js` service worker, fixed public `key` |
| `manifest.firefox.json` | MV3 manifest: `capture-model.js` + `background.js` background scripts, add-on id |
| `background.js` | Browser action opens `graph.html`; history and permission listeners; the bounded `storage.local` capture queue |
| `capture-model.js` | `GraphshellCaptureModel`: default policy, visit sanitization, queue bounds |
| `action-form.js` | `GraphshellActionForm`: builds an intent payload from an advertised `input_form` |
| `bridge.html`, `bridge.css`, `bridge.js`, `resource-chunk.js` | The native surface: connects `org.mere.graphshell`, pulls bounded resource chunks, renders identity cards and advertised actions |
| `prepare-extension.ps1`, `prepare-extension.sh` | Assemble one unpacked extension directory |
| `install-native-host.ps1`, `install-native-host.sh` | Register the native host for the current user |
| `native-host.chromium.json.in`, `native-host.firefox.json.in` | Native-host manifest templates; `__GRAPHSHELL_NATIVE_HOST__` is replaced with the binary path |
| `smoke-action-form.mjs`, `smoke-capture-model.mjs`, `smoke-capture-background.mjs`, `smoke-native-host.mjs`, `smoke-transfer-accept.mjs`, `smoke-resource-chunk.mjs` | Node smoke tests |

`prepare-extension` also copies `index.html` (as `graph.html`), `styles.css`,
`loader.js`, `mount.js`, `extension-profile.js`, the fonts and their notices
(`IBMPlexSans-Regular.ttf`, `IBMPlexMono-Regular.ttf`, `Newsreader-Regular.ttf`,
`OFL-IBMPlex.txt`, `OFL-Newsreader.txt`), and
`pkg/` from the parent `web/` directory into the destination. `dist/` is
gitignored, so it is the usual destination.

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

`nativeMessaging` and `storage` are installed permissions. `history`, `tabs`,
and `webNavigation` are optional; the portal's **Enable capture** control
requests `history`. The default policy in `capture-model.js` has `enabled:
false`. Visits are sanitized against the query-string and origin-exclusion
settings before they enter the `storage.local` queue, and the Wasm host
acknowledges them after one Muniment batch persists the graph document, the
`LocalOnly` AccessRecords, and the Eidetic browsing traces.

Capture attribution comes from the `SelectedPersonaRef` and device reference
injected by the composing host, not from the selected vault profile or a
device-roster entry.

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
[`../../djinn/install-windows.ps1`](../../djinn/install-windows.ps1)
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
