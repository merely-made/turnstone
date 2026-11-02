# Browser surfaces implementation: open work

**Date:** 2026-11-02
**Status (2026-11-02):** cut 2026-11-02 from [the browser surfaces implementation plan](archive_docs/2026-11-02/2026-08-25_browser_surface_implementation_plan.md), archived whole with its 1,165 lines of receiving history and inline run evidence. Landed in the archived plan: T0 contributed document surfaces, K0 one-gesture Keep, F0 retained find (accepted 2026-08-26), D0 permission and authentication decisions (accepted 2026-08-26 at the host boundary), E0.1 capability disclosure, E0.2 page zoom (2026-08-27), the coordinated repin at `abb349cf` with follow-up `8d1f907f` (2026-10-07), and the Linux, macOS Metal, physical IME, geometry, password and Taffy receiving sections of 2026-10-23 to 2026-10-29 (bounded to the hardware named in each). Open: A0, S0 and X0 (not started), the E0 tails, default Servo correctness, accessibility and platform qualification. Note on names: "S0" in the archived status block (L14, L167) is the supplier gate; "S0" below is the shallows.

## Rulings in force

Cited in the archived plan, not re-decided: the authority split (L471-476), the dependency order (L478-489), the page capture plan's D1 to D6 (L626-628) and the page lifecycle plan's L1 to L7 (L951-954), K0's one-way idempotent Keep (L508-514), page zoom as a document scale separate from UI and canvas zoom (L587-588, L599-601), `FrozenScene` stays a graph projection (L588-589), web policy and participant-grant authority stay distinct (L559-560), a typed method is not proof an engine emits the callback and CEF server authentication stays Partial until a live receipt exists (L775-777; `src/shell/weld.rs:419-421`), the terminology amendment of 2026-09-20 (L364-383), and the standing exclusion that a requested Linux/Vulkan backend is not evidence (L1075-1078). Old ids (T0, K0, F0, D0, E0, A0, S0, X0, R1a to R3) are stage names, not rulings, and stay grandfathered. DOC_POLICY section 11 applies forward from 2026-11-02; new decisions are keyed `browser_surface_implementation D<n>`.

## Decisions

### browser_surface_implementation D1: receipts leave the repository (2026-11-02)

Mark, 2026-11-02: "Move all, fixture to tests/". The receipts (`browser_supplier_integration_20261006`, `browser_family_20261007`, `browser_scry_windows_20261005`, `servo_cue_20261023`, `cue_ui_20261023`, `cue_ui_linux_20261023`, `cue_scroll_repair_20261023`, `cue_native_linux_20261023`, `cue_ime_linux_20261024`, `cue_text_input_20261024`, `cue_geometry_linux_20261027`, `cue_password_geometry_linux_20261027`, `cue_taffy_geometry_linux_20261029`) are in `Code/testing/turnstone/receipts/`, cited as paths.

Links: area receipts · applies DOC_POLICY section 8 · summary receipts moved to Code/testing/turnstone

### browser_surface_implementation D2: cut the plan, drafts first (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The plan is cut under DOC_POLICY section 8.

Links: area documentation · replaces 2026-08-25_browser_surface_implementation_plan · summary plan cut to open work

### browser_surface_implementation D3: hashes (2026-11-02)

Mark, 2026-11-02: "Live docs: local + sibling". `aac43a3f` and `679d8314` have no equivalent on main and stay as text. Mapping: [the receipts file](archive_docs/receipts/2026-08-25_browser_surface_implementation_plan_receipts.md).

Links: area documentation · summary live docs cite main hashes

### browser_surface_implementation D4: the gap analysis is archived (2026-11-02)

Mark, 2026-11-02: "Everything, drafts first". The historical [browser gap analysis](archive_docs/2026-11-02/2026-08-17_smolweb_browser_gap_analysis.md) is archived; this plan is its successor and carries nothing from it.

Links: area documentation · draws on 2026-08-17_smolweb_browser_gap_analysis · summary gap analysis archived

## Open work

### O1. A0: arrivals, custody and network inspection

An arrivals projection (feed unread, downloads and place additions by provenance), opening selects or opens the durable target, network rows keep request identity, phase, timing and public routing while redacting secrets and sensitive Gemini queries, and everything stays correct across session switches and restart (L638-647). Not started: `src/` has no arrivals projection or network-row model (only unrelated place-watcher comments, `src/place/lanes/join_live.rs:143`); `src/download.rs` and `src/feed.rs` are ingredients.

**Done when:** the four bullets at L638-647 hold with a headed receipt.

### O2. S0: the shallows

A persistence-admission boundary so restart proves unkept content never entered durable stores; Keep atomically promotes graph truth plus policy-selected representations or leaves both ephemeral; any lens projects the same shallows state; indicators are accessible and not colour-only (L659-666). Ruled by L1 to L3. Not started: no `shallows` identifier exists. Pitfall: styling a space without changing admission gives false privacy (L760-763).

**Done when:** L659-666 holds.

### O3. X0: extension completion

Finish three trust lanes (host command packs, portable `DocumentScript`, registered loaders and providers) with install, review, revoke and restart receipts per lane, typed visible provider-registration failures, and revocation that removes future authority without invalidating retained evidence (L675-681). Not started: no `DocumentScript` or command-pack code in `src/`. A document surface is never granted an app, renderer, device or filesystem handle by convenience.

**Done when:** L675-681 holds.

### O4. E0 tails

Capture: a captured page retains source identity and observed-state provenance and save or print consumes the captured representation (L602-604); P2 is at host correlation and the Weld capture hook and custody are open (the work is in [the page capture plan](2026-08-28_page_capture_plan.md)). Authentication: CEF 151 never called `GetAuthCredentials` for a top-level Basic challenge (L578-581) and the repin moved to CEF 154.5.0 (L148); no re-test is recorded. Servo import observation stays `Constructed` pending a host-import feedback seam (L1004-1007, L1040-1041).

**Done when:** E0 is complete when the capture and live-auth probes pass (L630-631); Mark decides whether to re-run the auth probe on CEF 154.

### O5. F0 acceptance debt (nonblocking)

Headed multi-match wrap and zero-match scenarios, project Previous, Next and Close into the accessibility find subtree, and host-edited find field behavior (paste, Delete, caret movement) when text-editing parity becomes the active slice (L548-556).

**Done when:** each is scheduled with a done-condition or Mark drops it.

### O6. Default Servo correctness and Scry gaps

Default-mode stale resized Servo pixels remain reproducible after the repin (surface and imported texture extents reach 509 px while the painted page stays 252 px wide) and the white reopened A is intermittent (L11-12, L185-186, L353-360); diagnostic positives do not qualify default B3 correctness. Graft's release gates (producer readiness, source reuse, resource-state handback, resource and fence lifetime) are supplier gates (L357-359). The Scry adapter gaps of the 2026-10-05 control (typed find and zoom, permission and authentication answers, correlated capture and PDF, DevTools, accessibility; L224-229) are historical; re-check against the supplier docs before copying. The descriptor soft limit needed a child-only budget of 65,536 (peak 2,333, L179-181).

**Done when:** default B3 correctness is qualified by pixel review on the default path (L15-16, L262-263).

### O7. Accessibility

Foreign browser-tree admission, ordered updates and tree-aware action routing; real joined-tree walks with Narrator or NVDA, VoiceOver and Orca (E1b, Mark's) (L289-292, L317-321, L362). Servo 0.7 forwards only Click and the producer never advertises it; CEF callback export and the Scry bridges stay Unsupported (L306-314). Physical assistive technology on all three systems is unqualified in every Cue section. AC6 cross-process transport is research (L301).

**Done when:** a portable joined-tree and action check with identity and lifecycle refusal controls passes, then the real walks per platform have receipts (L317-321).

### O8. Cue platform qualification

Physical IME on Windows and macOS, candidate popup, CJK methods, Servo IME on Linux, X11 and Xwayland, native high-DPI (scale 2 is a programmatic fixture only), physical device loss, transparent native colour and alpha and OS-compositor pixel correlation (L29, L100-102, L131-133, L1048-1050, L1137-1138, L1163-1165); other GPUs and compositors than the one ThinkPad (RADV Renoir/Vulkan, GNOME Wayland); fresh Windows Cue receiving and the Weld and Scry shared-native adapters (L1047-1049); package, registry-only consumer and hardware release gates (L277-278).

**Done when:** each carries a gate with the hardware named, or Mark drops it.

### O9. Residents R1a and R1b

R1a: the focused scenario receipt is pending, and forks do not copy delegation certificates (L407-408, L427-428); R1b: correlated port completion and a fallible graph-persistence receipt are required before claiming end-to-end durable effects; R2 and R3 are later (L449-452). `RunDenizen` for a vanished resident returns with no event; "surfacing it is a product decision" (L465-469; the cited `src/app/denizen_arms.rs` line is not confirmed).

**Done when:** Mark decides whether to surface a refusal event; the receipts are produced.

### O10. Identity and seed

The identity session's seed and vault repair is not a published baseline; the repin carries only the preview compatibility change (L158-161).

**Done when:** the repair is published and a repin adopts it, or Mark drops it.

## Findings

- 2026-11-02: the status block (L5-16) predates six later sections; the 2026-08-30 entry (L880-882) says the P1 compile gate is open, but it closed 2026-09-09 (L22, L627-628).
- 2026-11-02: the CEF auth gap is still declared in code: `src/shell/weld.rs:419-421` sets auth Partial and `:422` says correlated snapshots await product controls.
- 2026-11-02: the red `place::lanes::tests::a_partition_heals_and_both_sides_converge` was recorded as pre-existing (L582-584); the 2026-10-07 passing counts (681 Windows, 650 Linux, L176-179) do not mention it, so do not assume it still fails. Library test counts differ by platform and feature set and must not be compared.
- 2026-11-02: `R1a`'s `piccolo`-gated watch-scope revocation test is skipped by default `cargo test`; run `--features piccolo --lib resident` (L424-427).
- 2026-11-02: Linux physical IME is not a complete four-stage run on one executable (first three paths on A, repaired Sky on B, L84-89); display scale 1 only is exercised natively (L47-48).

## Progress

- 2026-11-02: plan cut from the browser surfaces implementation plan; decisions D1 to D4 recorded. The evidence index and old-to-main hash table are in the receipts file.
