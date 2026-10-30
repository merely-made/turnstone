# Graphshell host: the reference host's open phases and the one-tree cutover

**Status:** 2026-10-28: cut from `2026-07-27_graphshell_reference_host_plan.md` and `2026-09-25_graphshell_one_tree_plan.md`, archived in `archive_docs/2026-10-28/`. The tree migration inventory (`2026-09-27_graphshell_tree_migration_inventory.md`) stays in place and is this plan's inventory. Reference host: H0 to H3 and H5 to H7 are complete; H4 is complete except an operational tail that now belongs to the family update resident plan; the H6 product-path addendum is open at its S3 apply slice and its relay leg; H8 has not started; H9's first-party application door and Turnstone receipts client are complete; H10's DNS-SD half is open and its owner is in question. One tree: phases 1 to 3 are done and Mark has accepted the architecture; phase 4, the move, is unfinished: the retained tree mounts all five public pages in source and the legacy presenter is retired there, but a fresh Wasm build, the five pages and the complete live scenario wall have not been accepted.

**For:** Graphshell as Mere's useful, WASM-safe reference host (graph portal, Personae identity surface, browser-extension companion, application launcher, cross-device surface for addressed things) and the one retained Cambium tree that now hosts its browser page. Opened by Mark's product ruling of 2026-07-27 that Graphshell is the reference host and portal into the Mere ecosystem, and by his 2026-09-25 choice "One tree first" with "Enablers first".
Not here:
- H9's AI and MCP surfaces, and its three-adapter done condition (a user, an application and a granted agent inspect the same scene and invoke the same action): `2026-06-26_mcp_native_graph_plan.md`, which stays in place. An AI-facing door must not mount the Retinue endpoint's routes (see Constraints).
- H4's tail, real sign-out or reboot recovery of the resident and final retirement of the disabled Personae task: `2026-10-28_family_update_resident_plan.md`. The device resident moved to djinn, so its remaining acceptance is there too.
- The mounted mere panel (V2b step 5) and the complete 164-case canonical wall for Cleromancy C1: `2026-10-28_reservoir_plan.md`.
- Shared history, session saves and the later pan, selection and context-menu editor work, and R1 presentation rules: `2026-10-28_scenograph_editor_plan.md`. Declarations, channel registration, portable dynamics persistence and replay: `2026-10-28_dynamics_grammar_plan.md`.
- The viewer cap's amendments and browser presentation tenants: `2026-10-28_cambium_presentation_plan.md`. Tabard typography: `2026-10-28_appearance_plan.md`.
- The reachability rungs (peer discovery, cached dial hints, macOS as a discoverable peer, privacy lanes) that H6 and H10 lean on: `2026-10-28_browser_carrier_plan.md`.
- Turnstone's native acceptance of retained content and physical-input gates, and Turnstone's page capture plan: the Turnstone repository (the native-stack ruling that framed them is in the Turnstone suite composition and capability census). A Livery-owned line breaker over Parley (Genet Stage 1): a Genet plan in the Genet repository.

Receipts from this plan's runs go to `archive_docs/<date>/`, never into this
plan (DOC_POLICY §8). Evidence that matters is a test in the repository or a
line in Findings.

## Constraints

Reference host:
- **Graphshell is a reference host, not a web browser.** Turnstone owns browsing. Graphshell uses its own wgpu surface through NetRender and does not embed Turnstone or import the surface-embedding family (`grafting`, `scrying`, `welding`) or Servo hosting in its WASM profile. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:53-100`.
- **The resident is djinn, not a Graphshell binary.** Graphshell composes the session brokers djinn serves; the plan's older "permanent resident host" wording is corrected (see Open questions). From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:16-24,81-85`.
- **Projections are not authorities.** The graph projects public identity, grant and signing facts; signed evidence stays authoritative in Personae, Servitor or the owning ledger, and editing a projected grant grants nothing. Secret bytes never become facets or `Container` bodies; browser WASM never receives a seed, private key, vault root, private epoch, decrypted slot payload or unrestricted signing handle. Cross-app persona linkage stays opt-in. Encryption at rest protects a closed vault, not a compromised native process, and the GUI must not claim it identifies every local SSH-agent caller. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:102-198`.
- **History is an append-only record.** `visit.history` is a derived cache over `AccessRecord`, never the chronology's authority; access on another device appends and never rewrites. A shared abstraction needs a second consumer; a Graphshell-only adapter stays in the port until another application proves the boundary. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:232-285`.
- **Radios stay in Retinue.** The Retinue agent is a Graphshell management endpoint for radios, not an AI agent. Mark's 2026-10-24 harness ruling keeps an agent harness free of retinue and LXMF, so an AI-facing door such as `graphshell-mcp` must not mount this endpoint's routes. A new repository only if the agent becomes a standalone product. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:363-387`.
- **Transfers.** Replicate within one persona keeps ids and appends to the same history; copy into another graph, vault or persona mints ids and keeps `CopiedFrom`; move is copy plus a separately authorized source retirement, and a successful copy never deletes the source. Browser Muniment remains product truth; the host's iroh store is a recovery replica with explicit pins; pin release needs evidence of non-reference and fails toward retention; blob bytes cross the broker as explicit chunks of about 512 KiB under the native-message frame cap; `navigator.storage.persist()` is advisory and unpersisted storage is presented honestly; pairing is the authorization for same-persona replicate, bound to a `pairing_id`. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:927-1010`.
- **The personal lane's confidentiality boundary is the roster, not the projection filter.** A filter is presentation. Only blobs an accepted transfer releases are servable to an admitted browser, and the release set is refused over 64 MiB rather than truncated. Sealing (`sync.encrypted`) is per graph and opt-in. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1125-1131,1241-1250,2174-2176`.
- **Discovery, service admission and transit stay independent axes.** DNS-SD announcing, browsing and the surfaced service types are three separate owner settings, default off; a TXT record is the device's claim, never a graph assertion; browsing never implies admission, which stays Notochord's. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1478-1525`.
- **The evidence ladder is explicit.** Compile, unit, native integration, headed browser, two-device network and real RF are different claims; a TCP Reticulum run does not prove RF. User-controlled settings keep the data boundary out of hardcoded defaults. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1870-1913`.

One tree:
- **One tree, enablers first.** Graphshell's page is one retained Cambium tree; the canvas is a producer in it and the presenter folds into Cambium. A control a screen reader reaches today is never lost. The canvas's rendering, physics and picking stay Pictograph's. A control Cambium lacks is added to Cambium, never built inside Graphshell. From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:84-110,595-610,1315-1321`.
- **Phase 3 acceptance was architectural, not performance.** Physics is live work: the recorded windows used paused analytic layouts and prove neither simulation cost nor moving-graph behaviour. Pause freezes visible positions; Restore arrangement is a separate action; resume starts from held positions. From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:183-196`.
- **Phase 4 shapes already ruled.** The physics panel is a docked "Graph tools" region with explicit Apply buttons, collapsible disclosures and a width breakpoint; the remote session switch is two aria-pressed buttons, WebRTC only, on both routes; the tree's one canvas leaf rasterizes the canvas or the board scene by the session switch; producers expose their own accessibility semantics. From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:202-420`.
- **A result names its basis.** A performance result names its revision, graph, motion state, dimensions and machine load; old debug medians are not a current responsiveness verdict. Source retirement alone does not accept the cutover. From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:564-571`.
- **The presenter keeps selection, inspection and opening separate.** The adjacent inspector is the first site treatment; selection is keyed by occurrence id within one source binding, independent of visible Canvas members, and lasts for the mount; equal names never merge occurrences; no universal presenter crate and no second workbench or session owner. A live document face keeps its engine and session owner. From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:849-947`.
- **Viewer delivery caps hold.** The viewer Wasm cap is 3.50 MiB gzip per route artifact (ruled 2026-10-16); glue, mount and history keep their own caps; the site's first load stays under its separate 64 KiB cap; a cap is not raised and an oversized viewer is not deployed. From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:1211-1221`.

## Phases

### H4r. Personae surface residue
Native Firefox key import, desktop dialog providers beyond Windows (macOS and Linux), a configured carry root and broader carry management are still unproven after H4's other conditions closed. The two operational tail items of H4 are in the family update resident plan.
- Done when: not stated; owed.
- Needs: the djinn resident (`2026-10-28_family_update_resident_plan.md`).
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:824-825,839-841`

### H5r. Browser extension residue
Persona and device injection by the composing host is proven in one host; proving it in a second host is a follow-on. Favicon intake is a separate optional `tabs` permission and retention feature. The C4 consent gate for live capture is shared with Turnstone's page capture plan, with its default open: this H5's `HistoryCapturePolicy` is disabled by default while the archived capture plan's C4 shipped `Full`.
- Done when: not stated; owed.
- Needs: Turnstone's page capture plan (C4 gate, other repository).
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:914-925`; `archive_docs/2026-10-28/2026-07-03_archived_plan_tails_plan.md:1109-1116`

### H6a. Product path, S1 bytes: the relay leg
S1's byte layer is met over mDNS and survives a host restart, but "over relay with the relay leg on the Mac" is not proven: a bare node id is not dialable, the address book is per process, and the Mac's mDNS is dead. The experiment named the cached address rung (persist the peer's last known relay-tagged `EndpointAddr` on the paired-device record). The peer directory should separate configured from connected, since `reachable=1` is address-book membership.
- Done when: a blob put on one device is fetched byte-identical on a second through the existing pairing, over mDNS and over relay with the relay leg on the Mac, surviving a host restart between put and fetch.
- Needs: `2026-10-28_browser_carrier_plan.md` (reachability R1, cached dial hints); see the status question below.
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1030-1075`

### H6b. Product path, S3 apply: recovery pins and transfer lane limits
The device-to-device half of S3 is met; open are promotion of destination staging to a recovery pin, the blob-reference-set intent pair that pin release depends on (each browser profile publishes its reference set over the broker), the pin/GC distinction (today `put_bytes` tags every blob permanently and no GC is wired, so everything is kept forever) and batching staging operations. Smaller limits: the settings surface should say a transfer needs both the offer and blob-availability lanes; a device not told its own key projects every offer and offer counts are advisory; browser blob serving pre-loads up to 64 MiB and an async-trait refactor is the answer when a transfer exceeds it. Deferred beside it: cross-persona copy authorization UX, a hocket-handoff-style signed offline envelope, and Turnstone (which holds no MereHost) after the carrier plan's C2 rules where the session machinery lives.
- Done when: a selection chosen in the browser on one device lands merged in the browser on a second with blobs verified and ids per replicate semantics; chunk ordering, duplicate delivery, restart and resume, the recorded persistence grant and staging promotion are receipted fresh.
- Needs: H6a for the relay leg of byte availability.
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1148-1151,1152-1362,1364-1369`

### H8. The Retinue agent and constrained profile
A Retinue-repository package (`ports/retinue-agent`, planned) starts as a local stdio endpoint projecting attached radios, peers and announces, paths, queue, airtime, power and transit state, service offers and recent bounded activity, with typed intents for settings, announce, connect and bounded send. Iroh/p2panda carries the first large-file path; the Reticulum or direct-PHY profile starts with addresses, compact graph records and small payloads and grows only from measured byte, fragmentation, airtime, resume and energy results.
- Done when: browser Graphshell mounts a real Retinue agent, displays live radio facts, changes one owner setting through an attributed intent, transfers an address plus one measured bounded file over the constrained path, and the resulting graph, access and transfer records agree with the Iroh profile. Stop rule: a TCP Reticulum run does not prove RF, and the existing 4 KiB one-hop result does not prove arbitrary file transfer or routing.
- Needs: §12 decision 5 (first projection); the low-power managed network plan's bench proofs; verification wall items 8 and 9.
- Rulings: Retinue ruling (Constraints)
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:363-387,1424-1458,1909-1910`

### H10. The local network as nodes: DNS-SD service browsing
Mark, 2026-07-27: "I'd like to be able to easily find printers and surface them as local network devices in my mere." One Container per service instance (type, instance name, host, port, addresses, TXT records), provenance "observed on this interface at this time"; the same device on two networks is not one node unless something stable says so; a printer's default handler opens its `ipp://` or `ipps://` address through handler routing. The peer-discovery half moved to the reachability rungs (R0). `mere-transport` exposes no accessor to enumerate discovered peers, so a device list needs a transport accessor and a service list needs the DNS-SD browser.
- Done when: a real printer on the LAN appears as a node with its address and advertised capabilities; a second run recognises it as the same remembered device rather than minting a duplicate; the node greys when it stops responding instead of persisting as live; and browsing can be turned off with nothing left listening. Stop rules: no TXT record becomes a graph assertion; no device is identified across networks without a stable identifier; browsing never implies admission.
- Needs: the transport accessor; `2026-10-28_browser_carrier_plan.md` (R0 and the short DNS-SD service type); ownership per the question below.
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1478-1530,1864-1868`

### H10r. macOS peer and multicast residue
A macOS resident Mere peer must ship as a signed app with the local-network usage declaration or it is silently denied multicast egress. Still open and untested: running the peer once from Terminal.app, a real Developer ID signature, and a `log stream` capture of the policy verdict; Windows to Mac discovery stays blocked by macOS policy. IPv6 multicast sends fail on that iMac from process start (lower priority; IPv4 carries discovery), and a one-off `Delegation(NotYetValid)` on a stale ticket was never explained. The swarm-discovery socket-rebuild branch is kept and not upstreamed.
- Done when: not stated; owed.
- Needs: a signed macOS bundle (R0 in `2026-10-28_browser_carrier_plan.md`).
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1742-1766,1860-1862`

### G6, G7. Compose several applications; product pulls and constrained management
Received from the archived remote projection host plan. G6: one saved Graphshell workspace reopens two endpoint applications, keeps their truths separate and preserves its own arrangement and links. G7: Woodshed, Hocket and radio-management adapters with task receipts, and a constrained profile with measured byte budgets. Their completion gates the publication of the four unpublished `graphshell-*` crates. The capability model's sub-delegation (a depth-1 certificate issuing a narrower one to the viewer) was tracked in the same archived plan; its gate (G5b, G5c) is met.
- Done when: G6 and G7 hold as stated above; no separate condition is stated for publishing the crates, whose gate they were.
- Needs: H8 for the constrained profile; for G6, an application serving a route (S1 of [`2026-10-30_capability_door_plan.md`](2026-10-30_capability_door_plan.md)).
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:427-435`; `archive_docs/2026-10-28/2026-07-03_archived_plan_tails_plan.md:683-686,865-868,1277-1282`

### H-A. AccessRecord moves to Eidetic
The provisional `AccessRecord` stays Graphshell-local until a second live producer, intended to be Turnstone, uses the same fields.
- Done when: the record and schema move into Eidetic; both consumers are converted; the Graphshell-local definition is deleted; `BrowsingTrace/v1` is migrated as an import projection rather than kept as a second history.
- Needs: Turnstone as second producer; §12 decision 4 (schema fields beyond the minimum, frozen through Graphshell plus Turnstone evidence).
- Rulings: -
- From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:232-266,1934-1936`

### T4. The move: five pages and the acceptance wall
The controls, chrome and canvas become one tree; `web_gpu.rs` and `component.html` retire; `loader.js` mounts through the web host on every page; the scenario lane moves to the generic verbs. The working source mounts all five public pages through the retained tree and has removed the legacy presenter, component markup and scenario pump, but fresh product and viewer builds, every live and retained prior scenario, whole-frame capture review and accessibility checks have not qualified it.
- Done when: every scenario under `ports/graphshell/web/scenarios/` passes headed on the tree; each recorded run under `testing/mere/scenarios/graphshell-web/` that still describes live behaviour is re-run with captures reviewed whole-frame and the rest marked historical (the live inventory is 126 scripts plus two historical comparisons); `read_page` lists every control with its role and name; all five pages mount. Fresh Wasm, all five public pages and the complete live wall remain open.
- Needs: the inventory (`2026-09-27_graphshell_tree_migration_inventory.md`); the reservoir plan's V2b step 5 for the mounted panel.
- Rulings: one-tree rulings (phase 3 follow-up 2026-09-27; physics panel and remote session 2026-10-01 and 2026-10-02)
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:611-654,564-571,2087,2238-2257`

### T4a. Remaining product controls
The next bounded product target is the remaining detail actions (facets, relations, handlers, representation), then find and arrange and the other inventory panels (saved scene and transfer, projection, capture, practice). Native S5, saved-scene integration and the live site's viewer replacement are separate work; dense controls and small glyphs are usability work.
- Done when: each inventory panel passes its scenario and page gate on the tree, recorded here and in the inventory.
- Needs: T4; Cambium gaining any missing control.
- Rulings: -
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:33,587-593,1380-1382`

### T4b. Live physics and large-graph responsiveness
Live stepping, drag while moving, pause and resume, and settling, with frame pacing and simulation cost recorded separately. Paused rendering took about 1.5 seconds at 2,000 nodes and needs independent investigation. Mark found the 2026-09-30 physics slightly laggy but acceptable; a comparison must identify the layout and law, graph size and density, visible count and elapsed-step settings. Moving-graph hide and show and the intentional hidden-timing rejection are headed gates not yet run. An event-driven producer needs a renderer-completion signal before caching textures. The collision worktree `Code/worktrees/mere-canvas-elapsed` still exists and its retirement is pending.
- Done when: live stepping, drag while moving, pause and resume and settling are exercised headed with pacing and simulation cost recorded separately; acceptable responsiveness at 2,000 nodes "remains open".
- Needs: T4.
- Rulings: phase 3 follow-up rulings (2026-09-27)
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:183-196,1750-1752,1795,1823-1826,1836,1859-1878`

### T4c. Graph gestures and contextual commands
Wheel zoom about the pointer, background-drag pan, the shared command set and a searchable context surface are qualified on the viewer. Open: remove the always-visible navigation buttons only after command and gesture parity; touch mappings need their own acceptance, not an inference from the mouse ruling; saved-product command-view persistence stays a migration target under the Scenograph editor plan's SE31.
- Done when: a headed run verifies pointer-anchored wheel zoom without page scroll, background-drag pan, node drag, configurable wheel alternatives and outer-page scrolling; right-click opens commands while right-drag still selects (SE26); keyboard invocation, search, disabled reasons, selection context, Escape and focus return work through the accessible retained tree; wide and narrow captures show readable graph content. The toolbar stays until this passes.
- Needs: `2026-10-28_scenograph_editor_plan.md` (SE31).
- Rulings: SE23 to SE26, SE31 (scenograph editor plan)
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:725-798`

### T5. Site presenter: the four feature targets
The first site presenter (stable project faces, adjacent inspector, selection preserved through folds) is landed in shared main. Open are the four targets: disclosed occurrence state (a disclosure adapter for the read-only viewer, selection reconciled through Fold and checkpoint changes, stale bindings rejected); supported presentation (the resolver applied before measurement, a stable logical-pixel reference measurement with enter, hold and exit hysteresis, automatic representation and footprint adaptation); coherent realization (painting, picking, command targets and accessible boxes share occurrence geometry and update generation; camera fit, restore arrangement, reset, foreground retention, pins and layout locks stay distinct); host and delivery proof (wide, narrow and reduced-motion captures, an equivalent list, the frozen fallback, and performance measured by named graph, motion, supplier revisions, dimensions, adapter and load). A caption, viewport and chrome-composition contract keeps the fixture heading and full tool inventory out of site chrome.
- Done when: the four targets pass as stated in the source's tests (two selected projects, one hidden subject, one departed occurrence, two accesses to one Resource; paint-only effect separate from footprint change; hysteresis thresholds; occlusion and missing measurements).
- Needs: `2026-10-28_scenograph_editor_plan.md` (R1) for the rule model.
- Rulings: -
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:800-947,829-847,880-930`

### T6. Hosted groups and camera follow-ons
Larger resolved-build datasets, lens reason accounting, scenes saved and reopened with group state, and nested frame layout follow the bounded fold proof. Preserved-camera offscreen content is open, and very large graphs are limited by the 0.1 zoom floor.
- Done when: not stated; owed.
- Needs: T4a (saved scenes).
- Rulings: -
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:688-691,1536-1539,1566-1569`

### T7. Viewer delivery and live-site cutover
Live-site deployment, native and touch qualification, remote CI execution (repository Actions were disabled on 2026-10-13) and the site's first-load cap with its static, mobile and non-WebGPU reading remain the site-cutover gates. The viewer lacks Arabic, CJK and Thai glyphs; no expanded font repertoire or identical complex-script segmentation is claimed.
- Done when: the exact deployment recipe fits every cap with an automated regression gate and the viewer replaces the site's sandbox with static, mobile and non-WebGPU reading and failure fallbacks preserved.
- Needs: `2026-10-28_cambium_presentation_plan.md` (cap amendments); T5.
- Rulings: viewer cap 3.50 MiB per route (2026-10-16)
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:1203-1221,1263-1264,1300-1302`

### T8. Retained content and browsing presenter: unclaimed gates
The native retained-content path does not close physical assistive technology, protected personal custody, all media adapters or cross-platform acceptance; a completed browser download of the clip export is unobserved. The shared browsing presenter passes headed; emulated touch does not qualify physical phones, performance or another renderer, publication is separate, and fitted project faces stay a visual limitation. Scrollbar-fade timing is capture-limited.
- Done when: not stated; owed.
- Needs: physical devices; Turnstone for its own native gates.
- Rulings: -
- From: `archive_docs/2026-10-28/2026-09-25_graphshell_one_tree_plan.md:2146-2154,2168-2170,2185-2187,2233-2237`

## Open questions

- **How to carry the "permanent resident host" bullet** of the product ruling: amend it to name djinn as the resident and Graphshell as the GUI and session brokers it composes, or supersede it with a pointer to the device resident consolidation plan. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:81-85`
- **Who owns H10's DNS-SD half.** The plan says it "remains scoped here" and also that it is "owned by Murm per the Djinn plan's F3", which names Murm for browsing and advertisement and Djinn for the service registry. This plan carries H10 until that is settled. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1483-1488`
- **Is H6a's relay leg still open?** The reference host plan records the cached-address rung as not built, while the reachability rungs plan records cached dial hints (R1) as landed on the LAN with a genuinely remote receipt open. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1030-1075`; `archive_docs/2026-10-28/2026-08-03_reachability_rungs_and_privacy_lanes_plan.md:146-249`
- **§12 decision 1:** the final names of the Mere facade features (`graph`, `linked-data`, `canvas`, `workbench` are working names). From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1917-1918`
- **§12 decision 4:** the `AccessRecord` fields beyond the minimum; freeze through Graphshell plus Turnstone evidence, not conversation. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1934-1936`
- **§12 decision 5:** which Retinue projection is first, attached-radio management or Reticulum route and service state; choose the one with a real device receipt available. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1937-1938`
- **§12 decision 6:** which non-Windows OS-protected vault backend follows DPAPI; until then the portable passphrase backend is the honest profile. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1939-1941`
- **§12 decision 7:** how much caller and target context each SSH-agent carrier can authenticate; the approval UI shows explicit unknown fields. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1942-1944`
- **§12 decision 8:** which identity facet vocabulary promotes out of Graphshell after a second application consumes it. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1945-1947`
- **Sealing default and epoch retention.** Decision 9 (per-recipient encryption of the personal lane) is settled and sealing is opt-in per graph; it stays so "until somebody wants it on by default", and epoch pruning waits on the lane gaining compaction (it refuses without a checkpoint). From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:2150-2153,2174-2176`
- **Which plan holds the Personae carry and secret-sync lane**, which the source calls a separate high-assurance lane with its own enrollment, wrapping, revocation and recovery receipts. From: `archive_docs/2026-10-28/2026-07-27_graphshell_reference_host_plan.md:1391-1395`
- **Do Retinue's position disclosures PD1 and PD2 count as the radio fact surface** for H8's live radio facts: yes, and the live half becomes adapting them; or no, and it waits for a product to expose location facts. From: `archive_docs/2026-10-28/2026-07-03_archived_plan_tails_plan.md:669-672`
- **C4's default for live capture**, `Full` or disabled and opt-in, to be answered with Turnstone's page capture plan. From: `archive_docs/2026-10-28/2026-07-03_archived_plan_tails_plan.md:1114-1116`
- **Where the companion, smolweb, p2p and federation forward arc lives**: in this plan or in a home of its own. From: `archive_docs/2026-10-28/2026-07-03_archived_plan_tails_plan.md:915-919`

## Findings

- 2026-10-28: the desktop resident is `ports/djinn/src/bin/djinn.rs`; Graphshell ships no device-host binary. Decisions 2 and 3 of §12 are closed against the tree (the browser store is muniment's IndexedDB backend, `crates/eidetic/muniment/src/indexeddb_backend.rs`; the presenter is a thin Genet host adapter over `genet_render_host::RenderCore`).
- 2026-10-28: a bare iroh node id is not dialable and the address book is per process; a resident sync host that cannot receive a packet reports `reachable=1` because `known_peers` is address-book membership.
- 2026-10-28: `put_bytes` tags every blob permanently and no GC is wired; everything is kept forever until pins exist.
- 2026-10-28: macOS denial of multicast egress follows the binary, not socket age; an unsigned dev binary under sshd or launchd is not a viable macOS peer.
- 2026-10-28: `mere-transport` exposes no accessor that enumerates discovered peers; mDNS resolves an address only for a peer id already held.
- 2026-10-28: the tree has no `html` or `body`, so a page's base styles go on `:root`; a hidden browser tab draws no frames, so frame timing needs the window in front and the mirror is written at mount and on a reader's action.

## Progress

- 2026-10-28: cut from the reference host plan and the one-tree plan; nothing new landed.

## Received from the backlog holder, 2026-10-28

Items the archived-plan-tails plan held with no owner, given this one when the active plans were cut; each is as the holder recorded it, not re-verified.

- The unified document host's tails: the JSON-LD broadcast follow-on, a sighted-keyboard orrery focus stop, tiles to `platen-view`, N secondary orreries, and its condition 1. From: `archive_docs/2026-10-28/2026-07-03_archived_plan_tails_plan.md:583-588`
