# Browser surfaces implementation plan

**Status, 2026-08-25:** in progress. T0, contributed document surfaces, and K0,
one-gesture Keep, are landed. F0 retained find and D0 decision UI are next and
may proceed independently. This plan succeeds the
remaining-work portion of the historical
[browser gap analysis](2026-08-17_smolweb_browser_gap_analysis.md); the analysis
remains useful as research and acceptance evidence.

## Authority and order

Turnstone owns user-agent actions, durable graph truth, shell policy, and the
composition of surfaces. Genet engines own document behavior. Mere owns shared
graph and projection primitives. A browser feature crosses those boundaries
only through an existing port or a contract forced by a second consumer.

The dependency order is:

1. T0 contributed surfaces, closed.
2. K0 Keep, because later private-space promotion and curation depend on it.
3. F0 retained find and D0 decision UI. They may proceed independently.
4. E0 engine parity and capture, after the surface and decision contracts are
   explicit.
5. A0 arrivals and network inspection, consuming the same event and custody
   models.
6. S0 shallows, after Keep and persistence admission are real.
7. X0 extension completion, after grant decisions and contributed surfaces
   have proved their host boundaries.

## Phases

### T0. Contributed document surfaces

Landed in `1e9dde1` and `a37e6a1`. Turnstone now registers data-only surface
providers, admits provider-owned sessions behind a typed unavailable surface,
retains session identity by `PaneId`, routes input without provider access to
the app, and opens Knot documents through the shared pane path.

Done-conditions:

- Provider lookup and source-schema admission are explicit and unambiguous.
- Repinning evicts the old retained session.
- Focus, pointer, wheel, and key input reach the retained provider session.
- Knot is a real external consumer rather than a Turnstone-specific document
  model.

### K0. One-gesture Keep and the first regard state

Landed `Action::KeepNode { member }` as a captured, target-bound promotion. The
existing `keep` graph tag remains truth. `feed` stays a separate tag so keeping
a node does not subscribe it. Project the state through observation, the shared
action catalog, and the browser strip. The action is one-way and idempotent;
release and purge policy are not a hidden toggle.

Done-conditions:

- A focus change after intent creation cannot redirect Keep.
- A successful Keep writes graph truth, emits `node-kept`, requests a session
  save, and redraws.
- The strip offers **Keep**, then retains disabled **Kept** state.
- The contextual palette and automation catalog offer the same target-bound
  action and remove it after completion.
- A focused unit receipt and a headed scenario assert the observable state.

### F0. Retained find

Build find as a document-session capability returning a retained match list,
current match, structural label or role, and scroll target. The omnibar may
summon it, but it must not become a second command catalog. Hosted engines and
Livery documents implement the same host-facing capability without Turnstone
peeking into either DOM.

Done-conditions:

- Query changes update a retained match model rather than a one-shot probe hit.
- Next and previous wrap predictably and reveal the selected match.
- The match count and current structural label are observable and accessible.
- At least one Livery document and one hosted engine pass the same consumer
  tests.

### D0. Permission and authentication decisions

Turn the existing request events into visible, origin-scoped decisions. Web
policy and participant-grant authority remain distinct even if one Cambium
component renders both.

Done-conditions:

- Pending requests survive redraw and cannot attach to a later navigation.
- Allow once, standing allow, and deny lower to the exact request identity.
- Sensitive credential material never enters observation, logs, or graph
  truth.
- Restart tests prove which decisions persist and which deliberately do not.

### E0. Engine parity, page zoom, and capture

Complete the user-agent contract across registered engines. Page zoom is a
document scale and remains separate from UI zoom and canvas camera zoom. Page
capture freezes what the person saw; `FrozenScene` continues to mean a graph
projection and is not renamed into a page snapshot.

Done-conditions:

- Each registered engine reports support or a typed unavailable result for
  find, page zoom, capture, and navigation controls.
- Page zoom persists per node or engine policy without changing chrome scale.
- A captured page representation retains source identity and observed-state
  provenance.
- Save/print consumes the captured representation rather than a graph scene.

### A0. Arrivals, custody, and network inspection

Compose downloads, feeds, shared-place arrivals, and request diagnostics as
views over existing records. Do not create another history database. Network
inspection reads redacted request events and custody records rather than actor
internals.

Done-conditions:

- An Arrivals projection can distinguish unread feed entries, completed
  downloads, and shared-place additions by provenance.
- Opening an arrival selects or opens its durable graph target.
- Network rows preserve request identity, phase, timing, and public routing
  metadata while redacting secrets and sensitive Gemini queries.
- The pane stays correct across session switches and restart.

### S0. The shallows

Implement the shallows as a persistence-admission boundary. Appearance and
space placement may reveal that boundary but do not define it. Nodes and
representations remain ephemeral until the exact member is promoted through
Keep with a provenance decision.

Done-conditions:

- Restart proves unkept shallows content never entered durable session stores.
- Keep atomically promotes graph truth and the representations selected by
  policy, or leaves both ephemeral on failure.
- Any lens can project the same shallows state without turning it into a
  window-private mode.
- Private-state indicators are accessible and do not rely on color alone.

### X0. Extension completion

Finish the three trust lanes already named by the product: host command packs,
portable `DocumentScript`, and registered loaders/providers. Reuse D0 for
grant decisions and T0 for contributed UI. Do not grant a document surface an
app, renderer, device, or filesystem handle by convenience.

Done-conditions:

- Install, review, revoke, and restart receipts exist for each supported lane.
- `DocumentScript` has a real Turnstone consumer with bounded inputs and
  outputs.
- Provider registration failures are typed and visible.
- Revocation removes future authority without invalidating retained evidence.

## Findings

- **2026-08-25, live tree:** `App::available_actions` is already the common
  catalog for the omnibar, observation, automation, and contextual panes
  (`src/app/palette.rs`, `src/shell/drive.rs`). New commands must join there.
- **2026-08-25, live tree:** `keep` already means durable node curation and is
  also applied to feed sources; subscription is independently represented by
  `feed` (`src/feed.rs`, `src/app/feed_arms.rs`). Ordinary Keep must not imply a
  subscription.
- **2026-08-25, live tree:** UI zoom is live in the retained chrome appearance,
  while canvas zoom is a camera gesture. Neither is page zoom
  (`src/chrome_view.rs`, `src/main.rs`).
- **2026-08-25, live tree:** permission and authentication requests already
  carry exact identities in `AppEvent`; the missing work is retained decision
  state and lowering, not another request detector (`src/observe.rs`).
- **2026-08-25, live tree:** the probe path supplies a point text target, not a
  retained, navigable match set (`src/shell/drive.rs`). The gap analysis's
  reference to the Inspector as an existing find index overstates the seam.
- **2026-08-25, live tree:** `FrozenScene` freezes a disclosed graph scene
  (`src/frozen_projection_pane.rs`, `src/a11y.rs`). It cannot serve as page
  capture without collapsing two different artifacts.
- **2026-08-25, live tree:** contributed-surface admission and Knot consumption
  landed in `1e9dde1` and `a37e6a1`. That seam is available to D0, A0, and X0.
- **2026-08-25, consumer build:** advancing the stale Mere and Genet git
  packages exposed an uninherited Parley patch before Turnstone compiled.
  Genet now exposes its fork as a workspace package in `94a4dc4155b`, and
  Turnstone restates that root patch just as it already does for Taffy.
- **2026-08-25, clean consumer:** Genet intentionally carries a 0.0.1
  `layout-dom-api` name-claim package beside the live 0.1.0 package. A Git patch
  without a version was ambiguous outside the path-overridden checkout;
  Turnstone now selects exactly 0.1.0 in both the direct dependency and patch.
- **2026-08-25, clean consumer:** the focused Keep receipt passes with only
  pushed sources: Mere `11d996a6`, Genet `94a4dc4155b`, and Netrender
  `6f1a4fe70`. The resolved graph still contains parallel registry and Git
  islands for `genet-livery`/`buckram`/`livery`, plus Fleece 0.2 and 0.4.
  Current types do not cross those islands, so this is a convergence sidequest
  rather than a false failure of K0.
- **2026-08-25, consumer build:** Netrender's published 0.1.2 release commits
  remained on a cleanable release worktree while git `main` declared 0.1.1.
  That split gave Turnstone source-distinct `Scene` types. The release history
  and regenerated lock are now on Netrender main at `6f1a4fe70`; the stale
  worktree and branch are pruned, and Turnstone patches the renderer family as
  one source.

## Pitfalls and contradictions

- The historical gap map describes the shallows as a space property, but its
  decisive behavior is store admission. Styling a space without changing the
  persistence boundary would provide false privacy.
- "Keep" previously mixed general curation with feed-source retention. The
  shared `keep` tag is sound; feed scheduling must continue to require its own
  `feed` state.
- Find cannot be inferred from the Inspector's structural reading or the
  probe's first text target. Those are useful prior art, not the retained
  navigation contract.
- Save/print cannot reuse graph `FrozenScene` by name. Graph projection and
  observed page capture have different sources, replay properties, and privacy
  consequences.
- Web permissions and denizen or participant grants may share visual grammar,
  but their authorities and revocation rules must remain separate.
- A hosted surface must answer capabilities through its registered session.
  Turnstone DOM inspection would break engine modularity and the shared-device
  boundary.
- Do not hide the remaining registry/Git dependency islands with blanket root
  patches. Git Livery is 0.0.2 while the published `genet-livery` island asks
  for Livery 0.0.3; the owner manifests and releases must align first, then a
  clean consumer must prove that the duplicate island actually disappears.

## Synergies and sidequests

- K0 gives feeds, bookmarks, the regard, and shallows promotion one durable
  predicate without giving them one policy.
- T0's unavailable surface and retained-session machinery can host permission,
  arrivals, and extension views without adding pane-specific shell branches.
- F0's match model can serve accessibility and automation directly, avoiding a
  separate test-only search API.
- D0's decision component can share layout and interaction code with denizen
  grant review while keeping different typed actions beneath it.
- A0 can fold Steward custody, feed unread state, and place provenance into one
  arrivals view because all three already end in durable records.
- Network inspection should first be a redacted event projection. Export,
  HAR-like serialization, and performance analysis are sidequests until a real
  consumer forces them.
- Inline audio remains deferred until a host-owned playback seam and another
  document consumer force a reusable contract.
- Dependency convergence remains an owner-first sidequest: Mere Canvas should
  move from published `genet-livery` to the aligned Genet source, and Mere's
  Knot/search consumers should move from Fleece 0.2 to the current Fleece 0.4
  contract. Take each only when its real consumer test can catch a type split.

## Progress

- **2026-08-25:** reconciled the historical gap analysis against the live tree
  and sequenced the remaining work into dependent lanes.
- **2026-08-25:** accepted T0 as landed evidence from `1e9dde1` and `a37e6a1`;
  no duplicate implementation was started.
- **2026-08-25:** landed K0 with target-bound action, graph-tag truth, shared
  catalog and observation projection, and retained **Keep** / **Kept** strip
  state. Focus-drift and idempotence unit receipts pass; the offline
  `scenarios/browser_keep.scn` headed receipt is `RESULT ok` and its capture
  visibly retains the disabled **Kept** state.
- **2026-08-25:** repaired the published-source Genet-Livery consumer boundary
  by landing Genet `94a4dc4155b` and adding Turnstone's explicit Parley patch.
- **2026-08-25:** fast-forwarded Netrender's published 0.1.2 stack onto main,
  pushed `6f1a4fe70`, pruned its release worktree/branch, and unified
  Turnstone's renderer source family.
- **2026-08-25:** the external published-source gate resolved and compiled from
  outside Turnstone's checkout, then passed
  `keep_action_is_target_bound_idempotent_and_observable`. It also exposed and
  recorded the remaining non-blocking Livery/Buckram and Fleece version islands.
