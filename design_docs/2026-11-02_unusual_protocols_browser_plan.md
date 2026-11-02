# Unusual-protocols browser: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut 2026-11-02 from [the unusual-protocols browser plan](archive_docs/2026-11-02/2026-10-06_unusual_protocols_browser_plan.md), archived whole with its 2,422 lines of findings, rulings, progress and inline run evidence. Closed in the archived plan: S0 (repin as one tested set, 2026-10-07), SC1 to SC4 (transport seam, `PeltContent`, host history, Turnstone drives one controller per node), Errand adoption (2026-10-26), the native receiving continuation (2026-10-22), the scenograph C2 and vault-lock work (2026-10-08) and Hest H1 to H3 (2026-10-20). Open: S1 to S11, SC5 to SC7 and the Mere-side deferrals, the three-lane tail carried from the archived resource handoff, and the residuals below.

## Rulings in force

The archived plan's section 7 (L1068-1401) stays in force and is cited, not re-decided: U1 to U17 (U1 all three operating systems, U2 accessibility on Windows, macOS and Linux leads and Misfin and Gemini over Reticulum follow it, U3 the Mere halves of S1 to S4 belong to Mere's smolweb fidelity lane, U7 converge first, U8 live pixels research only, U9 tested-set order Knot, Redshank, Turnstone, U14 push order), AC1 to AC6, SC1 and SC2, the vault-lock shape (identity is `None` while locked, no fallback key minted, L1394-1401) and the SE45 to SE50 menu choices (L1741-1764). Page lifecycle rulings L1 to L3 stand (L1265-1274). Old ids are grandfathered. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `unusual_protocols_browser D<n>`.

## Decisions

### unusual_protocols_browser D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all, fixture to tests/". The receipts this plan cites (smolweb and NomadNet acceptance, `browser_family_20261007`, the Hest receipts, `native_browser_receiving_20261022`, `support_receipts/errand-adoption` and `support_receipts/client-limits`) are in `Code/testing/turnstone/receipts/` and `.../support_receipts/`, cited as paths.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/turnstone

### unusual_protocols_browser D2: cut the plan, drafts first (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The plan is cut under DOC_POLICY section 8.

Links: area documentation · replaces 2026-10-06_unusual_protocols_browser_plan · summary plan cut to open work

### unusual_protocols_browser D3: hashes (2026-11-02)

Mark, 2026-11-02: "Live docs: local + sibling". Main hashes here; the archived plan keeps its own. Hashes with several equivalent commits (`cab743c1`, `934494bf`, `af5c3ba85`) or none (`679d8314`, `211ff57a`, `f68af0d`) stay as text. Mapping: [the receipts file](archive_docs/receipts/2026-10-06_unusual_protocols_browser_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes

### unusual_protocols_browser D4: items carried from archived documents (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The archived resource handoff's "Next closure" list becomes O12; the archived q-pc handoff's item 5 (SC5/SC6/SC7 ledger drift) becomes O9.

Links: area documentation · draws on 2026-10-08_browser_resource_handoff · draws on 2026-10-14_qpc_lane_handoff · summary extracted items

## Open work

### O1. S1: every text document opens

Mere half: content-type lowering for `text/plain` and `text/markdown`, XML sniff to feed, `nematic.text` and `nematic.markdown` registered; Turnstone half: `route_filtered`, an explicit Misfin "not offered" tile (L548-559). Tree check 2026-11-02: the registered smolweb ids are still the original eight (`src/shell/smolweb_session_engine_ids.rs:7-16`), routing is the unfiltered `route` (`src/shell/effects/shell_effect_spawn_content.rs:145`) and `nematic.text`/`nematic.markdown` appear only as labels (`src/content/engine_label.rs:16,18`). Whether Mere's `pelt_core::native::sessions_with_fetcher` registers them is not checked.

**Done when:** the five failing-first cases (Gopher text, Gemini text/plain, Gemini text/markdown, Misfin link, RSS as application/xml) pass and a headed local Gopher receipt records engine ids (L561-572). Gate: Mere's fidelity lane for its half (U3).

### O2. S2 reading basics, S3 trust posture

S2: keyboard-only following of three links on a local Gemini receipt, find counts and steps, page zoom persisting per node across restart, selected text reaching the clipboard, capability disclosure reading supported (L740-747); needs Mere's find, zoom and selection on smolweb sessions. S3: Spartan reads unauthenticated, Gemini shows its pin state, NomadNet destination-proven, no successful load reads Unknown (L764-770); gated on Mere's WS2.

**Done when:** each stage's quoted done-condition holds with a dated receipt.

### O3. S4 Gopher and the small lanes

Aligned Gopher columns, type-7 search returning results (Turnstone must stop refusing the Gopher type-7 Submit, L198-203, L535-537), Finger, Nex and Guppy each with a test and fixture receipt, a feed receipt that subscribes, refreshes, marks read and opens an entry (L783-792). Gates: WS4 (pushed 2026-10-07) and `gopher-protocol` 0.2.0 published on Mark's sign-off (L774-775).

**Done when:** L783-792 holds.

### O4. S5 NomadNet without environment variables, S6 settings and home, S7 offline revisits

S5: the interface is still read from `TURNSTONE_NOMADNET_TCP` (`src/nomadnet/link.rs:104-107`); configure it in Settings, take a stock node from an announce, open the index, submit a form, download a file, and keep interface, identity and directory across restart (L808-814); gate U5. S6: start view as a setting, home pane by default, omnibar "search with <capsule>" (L834-843); gate U6; no start-view code found. S7: offline same-run pages open marked stale, captured pages open after restart, unkept pages say they need the network, no unkept body on disk (L861-869); gate U10 and the offline replay in [the page capture plan](2026-08-28_page_capture_plan.md).

**Done when:** each stage's done-condition holds on a fresh profile with no environment variables set.

### O5. S8 and SC5: live document accessibility

Publish the controller projection into the composed forest, route actions with identity and placement admission, migrate the contributed trees to the shared helper (L882-891, L1987-1993). The `assert a11y` scenarios exist for Gemini, Gopher, Micron and the lens (`scenarios/document_a11y_*.scn`); the three-OS human receipts do not. Gates: physical Windows, macOS and Linux assistive-technology receipts; Genet's `AccessKitBridge` dropping an action's `target_tree` (affects standalone Pelt, not Turnstone, L1514-1517); bridge activation epochs and native window bounds (L2097-2099); E1b headed walks with Narrator or NVDA, VoiceOver and Orca on Fedora and Mint, which are Mark's (L1512-1513).

**Done when:** a person reads, follows a link and submits a field with a screen reader on each of the three systems, with a dated receipt (L892-898).

### O6. SC6 pool and placements, SC7 store traits

Engine-aware pool, repeated placement geometry, Reader appearance lifetime and the shared surface input contract (pointer identity, wheel modifiers, button parity) (L2002-2004, L2021-2023); the stale-action gate is "under qualification, not completed" (L2053-2055); SC7: reusable store contracts below product conversations (L2004-2006). `src/shell/content_pool.rs` (394 lines, `HostPlacement`) exists, so SC6 is partly in source though no plan line says it landed.

**Done when:** the SC done-condition (L598-605) holds: Turnstone and Pelt drive document tiles through one controller, Stop/Reload identities are unchanged and S1's five cases still pass.

### O7. SC deferrals in Mere

`PeltWorkspace` passthroughs for host loading, Pelt desktop's switch to the `mere-fetch` actor and trait-level `replace_body` in Genet (L1575-1577). Mere and Genet own them.

**Done when:** Mere's plans carry them or Mark drops them.

### O8. S9 distribution, S10 bar acceptance, S11 composition

S9 (gate U1, all three systems): package installs and browses on a clean Windows account, updates and rolls back, a forced panic is recorded and restored (L914-920); no panic hook or packager in the tree. S10: an installed build with no environment variables browses each U2 protocol against a local fixture with a dated receipt, and Mark rules the bar met (L926-930). S11a Scenograph over Turnstone's graph and S11b the Woodshed practice surface (L936-946); S11b needs one process-wide owner of the audio device.

**Done when:** each stage's quoted done-condition holds.

### O9. Plan ledger drift: SC5/SC6/SC7 (from the q-pc handoff)

The archived plan states SC5/SC6 as open (L1985) while its status says the resource handoff and pumping are "qualified at Turnstone `25f85bd`" (L14-15) and its 2026-10-09 pass only records RED receipts (L2010-2035). The q-pc handoff asked to "reconcile implemented versus qualified scope".

**Done when:** each of SC5, SC6 and SC7 is stated here as landed, qualified (with the receipt) or open, with the code that shows it.

### O10. Misfin, Gemini over Reticulum, browser sidecar scope

Misfin (through Mere comms and errand work) and Gemini over Reticulum come after the accessibility bar (U2); no stage or done-condition exists (L479-480, L1231-1233). The browser sidecar scope (four proposed cuts, L1039-1060) is a proposal, not an authorization (L1061-1066); its product entry point needs reconciling with the Graphshell reference-host plan (L978-980).

**Done when:** Mark rules whether either becomes a stage; each gets a done-condition then.

### O11. U8 live-pixel research and AC6 transport research

U8: "start research, don't start integration" for live pixels (L1203-1207); AC6: cross-process transport is research after AC1 is proven (L1332-1335). No result is recorded.

**Done when:** a dated research note exists for each, or Mark drops them.

### O12. Three-lane tail (from the archived resource handoff)

The 2026-10-11 checkpoint said "Integration remains open": final-source native Graphshell inventory and full suite (including 22 Forme and seven panel/paging regressions), Turnstone default and Piccolo gates, fresh native captures, current Graphshell Wasm headed acceptance, diagnosis of the strict Scene RED, then coherent Genet, Mere and Turnstone pins with portable resolution proven and a whole-change review. It is Mere- and Graphshell-side work; the canonical plan is Mere's (L1998-2001). Graphshell's five public mounts keep their continuation (L15-17).

**Done when:** Mere's plan reports the five-page cutover qualified, or Mark hands it to Mere and drops it here.

### O13. Residuals of Errand, native receiving and Hest

Errand adoption: headed UI, live certificate decision flows, actor cancellation, physical radio and independent-machine qualification keep their gates; Clippy has 278 warnings in unchanged source (L37-39, L59-65). Native receiving: installed application, window/GPU/headed, independent Python and Go readers, buffered Scroll body limits, aggregate browser memory and traffic, HTTP wire-header limits, download and custody policy, rich Scroll response headers, client-certificate UI (L2156-2158, L2209-2210, L2255-2274). Hest: fresh remote-only resolution waits for authorized delivery of Knot, Mere then the consumer commit; native GUI, restart and human assistive-technology acceptance; crate publication held; H4 not implemented (L2366-2369, L2418-2419).

**Done when:** each carries a gate in the owning plan, or Mark drops it.

### O14. Command-menu and vault-lock follow-ons

Headed check of right-drag selection and the menu (scenarios are Windows-gated) and Keep/Drop control font zoom scaling (L1726-1728); headed check of the SE45 to SE50 adoption (L1767-1770); a Windows headed run for the surface lane (L1895-1897) and for SC pass A (L1690); mDNS seed-copy exposure waits for the identity session's fix in mere-transport (ruling 60, L1623-1627); a vault lock while Turnstone runs is learned only through Djinn's broadcast (L1834-1835).

**Done when:** each is run or handed to its owner.

## Findings

- 2026-11-02: the archived plan's status mixes dates: the line at L14-15 says qualified, the 2026-10-09 pass at L2011-2019 says "resumes". Section O9 settles it.
- 2026-11-02: pins quoted in the archived plan are historical; the root manifest now tracks `branch = "main"` rows (`Cargo.toml:52`, `:63`, `:258`, `:312`).
- 2026-11-02: Turnstone refuses any score whose `sceno::SCORE_VERSION` differs from its own (`src/remote_projection/impl_projection_source_for_turnstone_endpoint.rs:16`).
- 2026-11-02: smolweb sessions report find, page zoom and page capture unsupported and have no selection or copy (L159-163); every Nematic engine sets `DocumentTrustState::Unknown` and nothing fills it (L168-170).
- 2026-11-02: dropping a completed Resource session aborts the interface writer before proofs and encrypted close are sent; the page helper must drain its endpoint (L2257-2268).

## Progress

- 2026-11-02: plan cut from the unusual-protocols browser plan; decisions D1 to D4 recorded; the resource handoff's tail and the q-pc handoff's ledger item carried as O12 and O9. The evidence index and old-to-main hash table are in the receipts file.
