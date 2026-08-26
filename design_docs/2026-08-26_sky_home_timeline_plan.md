# Sky home daily timeline plan

**Status, 2026-08-26:** in progress. P0's pure timeline and validated derivation
receipt are complete. Both the exact-Turquet source harness and the full
Turnstone graph gate pass. Pane registration, retained Cambium presentation,
and headed acceptance remain open as P1 and P2.

## Purpose and authority

T5a is Turquet's first real astronomy-consumer gate: a daily Sky view composed
from observer-relative positions, lunar phases, conventional rise and set, and
upper and lower meridian transits. The consumer belongs in Turnstone because a
daily home is product presentation and policy. Turquet owns the astronomical
facts, typed model revisions, event intervals, and calculation accuracy. Genet
and Cambium own the retained rendering and input machinery. Mere has no role in
the calculation or the surface.

This work consumes Turquet only through its public API. Turnstone does not copy
astronomical formulae, infer facts from display strings, or reach into provider
implementations. The first consumer pass is expected to exercise two small
public seams being completed in Turquet: converting a typed TT Julian date back
to a scale-aware epoch and asking a geocentric position provider for its
accuracy declaration. See `turquet/ROADMAP.md`, T5a, and
`turquet/design_docs/2026-08-13_provider_architecture.md`.

Work began in `Code/worktrees/turnstone-t5a-sky` because the ordinary Turnstone
checkout carried an unrelated retained-find lane across app, shell,
accessibility, and scenario paths. Nothing from that dirty checkout was copied,
staged, or rewritten. The Find lane landed as `c339772` before this slice was
integrated; later pane registration can now proceed from that shared baseline.

## Civil-day contract

A Sky request names an observer, a local calendar date, and an IANA time-zone
identifier. The selected day is the half-open interval from the named date's
zone-resolved start to the following date's zone-resolved start. Boundaries use
Jiff's compatible named-date resolution, so a skipped midnight advances to the
first valid instant belonging to that date. The consumer then converts both
boundaries to UTC and TT. It never manufactures a 24-hour interval, so
daylight-saving transitions naturally produce 23- or 25-hour days. An event
belongs to the day when its TT midpoint maps inside the half-open local
interval. A result exactly at the following boundary belongs to the next day.

Named-zone resolution and civil labels are Turnstone policy. Turquet continues
to receive typed TT bounds and an explicit Earth-orientation provider. The
request also names the body set, an anchor instant for position facts, and the
caller-selected conventional rise/set policy. Unlike calendar-day boundaries,
a caller-selected local anchor must resolve uniquely; gap and fold anchors are
rejected. Defaults, if presented later, remain user-configurable rather than
engine law.

## Projection and receipt

The pure model produces a sorted list whose variants remain explicit: position
snapshot, lunar phase, rise, set, upper transit, and lower transit. Each entry
keeps its TT interval or epoch and the source fact needed to explain it. Local
time is a projection for ordering and display, not replacement authority.

One validated derivation manifest accompanies the list. It records the exact
Turquet package revision, civil-day request, resolved UTC and TT boundaries,
observer, selected bodies, provider model and accuracy, provider data snapshot
when present, Earth-orientation authority and snapshot, rise/set policy, and
every raw result used by the projection, including per-fact models and tagged
details. Its DTO round-trip is exact and its pretty-JSON bytes are stable for
the same inputs. It is not a claim that JSON alone can recompute the astronomy.
Display formatting and later layout changes must not alter it.

## Exclusions

T5a does not add weather, terrain or obstruction models, generalized
visibility judgment, local eclipse circumstances, illumination or distance
extrema, named twilight, astrology, notifications, graph persistence, or live
location permission. It does not promote a new shared crate. Pane persistence
and a configurable settings source belong to later host wiring.

## Phases and done-conditions

### P0. Pure consumer model

- A validated request resolves a named-zone half-open civil day to UTC and TT.
- A configurable body set, with Sun and Moon as the first receipt, composes an
  anchor position plus conventional rise/set, phase, and meridian events using
  public Turquet calls only.
- The result is chronologically sorted by instant while retaining TT bounds,
  interval width, body and event kind, provider/EOP provenance, accuracy, and
  the complete rise/set policy.
- Tests cover the Boston 2024-04-08 receipt, one polar empty-result control,
  and 23- and 25-hour America/New_York civil days.
- A provider spy proves that the consumer uses the provider seam rather than
  calling the analytical implementation directly.

### P1. Contributed Sky surface

- `turnstone.sky` is admitted through the existing `SurfaceProvider` registry
  from a versioned source schema, retained by `PaneId`, and summonable through
  the common action catalog.
- Date, zone, observer, body selection, Earth-orientation disclosure, and
  rise/set policy are accessible and inspectable; a change rebuilds one
  timeline and receipt without creating a second authority.
- The view distinguishes bounded event intervals from formatted local labels
  and makes empty results explicit without claiming always-visible or
  circumpolar status.

### P2. Acceptance

- Focused model and surface tests pass against an exact committed Turquet
  source revision rather than a local path redirect.
- A semantic headed scenario changes the day or policy, observes the new
  timeline and receipt, and captures the presented frame.
- The active documents and Turquet roadmap distinguish the landed consumer
  from the still-open local-eclipse, extrema, twilight, visibility,
  Cleromancy, and embedded gates.

## Findings

- **2026-08-26:** Turnstone's landed contributed-surface seam already owns
  source admission, retained session identity, layout, scrolling, hit testing,
  and input routing. Knot's provider is the concrete adapter pattern. T5a does
  not justify changing Mere or Genet.
- **2026-08-26:** The existing dirty retained-find lane overlaps the future
  registration and headed paths, so P0 used worktree isolation. The lane later
  landed as `c339772`, clearing that integration boundary.
- **2026-08-26:** Civil-day membership cannot be represented honestly as a
  fixed 24-hour offset. Named-zone boundary resolution is part of the consumer
  contract. Calendar boundaries use compatible resolution for skipped
  midnights, while caller-selected anchors remain strict and reject gaps and
  folds.
- **2026-08-26:** A position sample is not a Turquet event. Turnstone therefore
  retains raw `EventInterval` values on event rows and uses its own
  zero-width-capable `SkyFactSpan` only in the derivation receipt.
- **2026-08-26:** The civil-time layer uses Jiff 0.2. The Chrono maintainer's
  [soft-deprecation issue](https://github.com/chronotope/chrono/issues/1768)
  recommends Jiff as the maintained alternative. Jiff's
  [platform documentation](https://docs.rs/jiff/0.2/jiff/_documentation/platform/index.html)
  records the relevant deployment behavior: Unix normally reads the system
  IANA database, while the default Windows feature embeds it and therefore
  requires a crate update and rebuild to receive database revisions.
- **2026-08-26:** A stable receipt cannot rely on serde_json's default fast
  floating-point parser: Boston's real observer and refraction values returned
  one ULP away from their serialized values. Enabling serde_json's
  `float_roundtrip` feature makes the two-part TT values and caller policy
  round-trip bit-for-bit. The fixed Boston receipt digest catches later byte
  drift.
- **2026-08-26:** A fresh branch-tracked Genet resolution sees both the live
  `genet-livery` 0.0.2 crate and its 0.0.1 name claim. Turnstone's direct and
  registry-patch declarations now select `=0.0.2`; this is required before a
  clean published-source consumer can reach compilation.
- **2026-08-26:** Receipt-wide ephemeris, observer-transform, and EOP metadata
  are deduplicated only after every returned event is checked against those
  identities. Provider accuracy remains an angular state bound, while each
  event span remains a solver bracket; neither is presented as event-time
  error.
- **2026-08-26:** Reading a durable V1 manifest does not re-resolve its civil
  day against the machine's current IANA database. Canonical stored UTC and
  two-part TT boundaries remain the historical authority; the named-zone label
  is syntax-checked identity, and current-rule comparison would be a separate
  operation. Strict DTO parsing rejects unknown fields and noncanonical fact
  ordering.

## Progress

- **2026-08-26:** Audited Turquet, Turnstone, Mere, and Genet ownership and
  selected Turnstone's contributed-surface host.
- **2026-08-26:** Founded this active plan before code changes and recorded the
  pure-model-first integration order.
- **2026-08-26:** Implemented the initial pure request, named-zone day resolver,
  typed timeline projection, and provenance receipt in `src/sky_timeline.rs`.
  A narrow verifier against the local Turquet 0.13.0 seam passes four focused
  tests: provider composition and provenance, unknown/duplicate request
  rejection, and the New York 23- and 25-hour DST days.
- **2026-08-26:** Completed P0's analytical Boston 2024-04-08 receipt: Sun and
  Moon positions, conventional rise/set, upper/lower transits, and New Moon
  produce eleven ordered facts. The exact pretty-JSON receipt digest is
  `caff8371d348ba141397a8185e291c533c6ab12d4fe85f9ce3be797707ce411d`.
  A Tromso solstice control returns both transits and no sampled conventional
  rise/set crossing under its selected center/no-refraction/level policy,
  without promoting that empty result into a visibility classification.
- **2026-08-26:** The isolated source harness passes sixteen tests against exact
  Turquet revision `bc3c454f755d0bfd70ab48bd9556a1cda2213d41`, including
  receipt byte stability, validated DTO round-trip, strict schema/order and
  tamper rejection, tzdb-independent reading, civil-day transitions and strict
  anchors, provider composition, Boston, and Tromso. The full Turnstone graph
  gate passes the same sixteen tests with 347 unrelated tests filtered; its 51
  warnings are the pre-existing branch baseline. All P1/P2 surface work remains
  open.
- **2026-08-26:** Replaced Chrono and Chrono-TZ with Jiff 0.2 without changing
  the named-zone half-open-day rule or its rejection of gap and fold anchors.
  The four-test focused verifier remains green, including both New York DST
  day lengths and provider receipt identity.
