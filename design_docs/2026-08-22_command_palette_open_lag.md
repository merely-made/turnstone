# Command Palette Open-Lag Note

**Date:** 2026-08-22 (attributed and fixed 2026-08-23, closed 2026-08-24)
**Status:** Closed. The cost was `ChromeView::scene` re-cascading the whole
chrome subtree every frame. Chrome now retains its layout per window-root; the
open-versus-closed ratio fell from 4.37x to 1.19x, and a per-surface raster
split then showed chrome repainting **zero** tiles per settled frame in both
states. The done condition is met. One unrelated question the split exposed is
named at the end.
**Scope:** Turnstone chrome and frame production.

## Observation

Interaction lags while the command palette is open. The symptom is tied to the
open state, not merely the opening transition.

Do not assign this to Knot, Graphshell, or the device resident from proximity.
Those systems may be excluded or implicated by profiling, but the first owner
is Turnstone's chrome path.

## 2026-08-22 diagnostic

A disposable unoptimized development build, compiled with debug information
disabled, reproduced the open-state cost at 1024 x 600. Two controlled redraws
with one surface took 25.9 ms and 26.7 ms. Four controlled redraws with the
palette open and two surfaces took 239.3 ms, 241.1 ms, 241.4 ms, and 250.7 ms.
The application remained responsive and the palette rows updated correctly.

This is approximately a 9x whole-frame delta in that build. It is evidence for
the reported symptom, not a performance baseline: the build was unoptimized,
the sample is small, and whole-frame logging does not attribute the time. The
profile used the ordinary sample graph rather than the resident Knot route, so
the diagnostic does not implicate resident work.

## 2026-08-23 release receipt

One release build (cargo's default release profile; this tree had never been
built in release before), 1024 x 600, four conditions, three runs each, on a
fresh profile per run. Idle conditions measure 120 settled frames per run, row
selection 50, text edits 45. Offline throughout: `mere://` addresses fetch
nothing, so no network or resident lane is in the numbers.

Whole-frame medians, with p95 in parentheses:

| Condition | frame_ms | chrome_scene_ms |
| --- | --- | --- |
| palette closed, idle | 10.162 (14.163) | 4.608 (5.951) |
| palette open, idle | 44.427 (50.964) | 36.509 (41.300) |
| palette open, row selection | 43.483 (51.327) | 35.474 (40.524) |
| palette open, text edits | 42.392 (51.397) | 35.665 (41.856) |

The whole-frame ratio is **4.37x** at the median. Optimization halves the debug
build's ~9x; it does not remove it.

Per-stage delta, open idle minus closed idle, median ms:

| Stage | Delta |
| --- | --- |
| `suggestions_ms` | 0.000 |
| `chrome_sync_ms` | 0.034 |
| `chrome_scene_ms` | **31.901** |
| `pane_scenes_ms` | 0.911 |
| `raster_ms` | 1.159 |
| `compose_ms` | 0.103 |

**93% of the delta is one stage.** Everything else is within noise of the
control.

## What the candidates turned out to be

The four candidates this note recorded on 2026-08-22, now answered:

1. **`recompute_omnibar_suggestions` on every palette edit - exonerated.**
   Median 0.000 ms in every condition, p95 0.057 ms in the worst one. A merely
   open palette does not recompute at all: `suggestion_runs` is 0 across all
   360 measured open-idle frames. It was the most intuitive suspect and it is
   not the cost.

2. **`ChromeView::sync` - exonerated. `ChromeView::scene` - confirmed.** The
   candidate named both halves in one breath; they turn out to be 0.034 ms and
   31.901 ms. Rebuilding the row views and updating every window projection is
   cheap. Turning the subtree into a scene is the whole problem.

3. **The chrome subtree has no `RetainedLayout` - confirmed, and it is the
   mechanism.** [`ui::scene_from_subtree`](../src/ui.rs) builds a fresh
   `LiverySnapshot` and a fresh `TextSystem` on every call, and
   [`ChromeView::scene`](../src/chrome_view.rs) calls it once per frame per
   window with no retention of any kind. Fourteen panes hold a
   `crate::ui::RetainedLayout`, whose own contract says `rebuilds()` is
   "expected to reach 1 and stay there for a pane that is merely being
   repainted". The chrome paid a full rebuild every frame instead. Fixed
   below.

4. **Whole-frame logging did not separate the stages - closed.** It does now;
   see the instrument below.

## The finding the receipt added

**The palette does not introduce the per-frame rebuild. It enlarges it.** With
the palette closed and nothing happening, the chrome still syncs once per frame
(360 syncs across 360 settled frames), still spends 4.6 ms rebuilding its
scene, and still repaints about 6 tiles a frame. Opening the palette makes that
subtree big enough for an always-present cost to become a visible one.

That reframes the fix. Retaining chrome layout is not a palette optimization;
the palette is the case that made an unretained chrome path impossible to
ignore.

Two smaller things the counters surfaced, neither of them the lag:

- A text-edit block that typed 8 characters recorded 18 suggestion
  recomputations, roughly 2.25 per keystroke, with `suggestion_refits` at 0 -
  so the doubling is not the fitted-row-limit refit path but something calling
  the recompute more than once per edit. It costs approximately nothing today,
  which is exactly why it would never be noticed without a counter.
- `ui::hit_test_subtree` built the same full snapshot the same way, so every
  hit test against chrome paid a cascade and a layout — on every pointer press
  anywhere in the window, including presses that miss chrome entirely, since
  `deliver_press` asks chrome first. Unmeasured here; fixed below, because it
  turned out to share not merely a fix but the very same retained layout.

## The instrument

`turnstone::shell::render` emits one `frame` line per presented frame carrying
the whole and its parts:

```
frame frame_ms=... surfaces=... palette=... suggestions_ms=... suggestion_runs=...
      suggestion_refits=... chrome_sync_ms=... chrome_scene_ms=... chrome_syncs=...
      pane_scenes_ms=... raster_ms=... rasterized=... dirty_tiles=...
      graph_rast=... graph_tiles=... content_rast=... content_tiles=...
      pane_rast=... pane_tiles=... divider_rast=... divider_tiles=...
      chrome_rast=... chrome_tiles=... compose_ms=...
```

`rasterized` and `dirty_tiles` are derived from the per-class fields rather
than counted beside them, so the parts cannot drift from the whole.

Enable it with `RUST_LOG=turnstone::shell::render=debug`. The accounting lives
in [`frame_timing.rs`](../src/frame_timing.rs); it stays compiled in, because
an instrument that has to be re-added is one that will not be there the next
time a frame feels heavy.

Two things about it are worth knowing before reading its output:

- **Suggestion time accumulates between frames.** It is spent on the input
  edge, inside `App::recompute_omnibar_suggestions`, not inside `render`, so a
  frame-local timer would read zero on the very keystroke that provoked the
  work. The accumulator attributes each edit to the frame that presents its
  result.
- **The counters answer a question the durations cannot.** The done condition
  below is about repeated work, not slow work. A duration cannot tell one
  expensive sync from four cheap ones; `chrome_syncs`, `suggestion_runs`, and
  `dirty_tiles` can, and `dirty_tiles` comes from netrender's own per-surface
  count rather than an estimate.
- **A summed counter names a symptom and hides its owner.** The first pass
  summed `dirty_tiles` across the frame, which was enough to say "something
  still repaints" and not enough to say what — and that gap is what kept a
  clause of the done condition open for a day. `<class>_rast` beside
  `<class>_tiles` is the fix: a class that rasterized and repainted nothing
  reads `1` and `0`, which is a different fact from a class that was not in the
  frame and reads `0` and `0`.

## The receipt

Executable, and offline:

```powershell
./scenarios/run_palette_lag.ps1 -TurnstoneBin ./target/release/turnstone.exe
```

Four single-condition scenarios - `palette_lag_closed.scn`,
`palette_lag_open.scn`, `palette_lag_keys.scn`, `palette_lag_edit.scn` - run
three times each on a fresh profile, parsed into per-stage median and p95 plus
the per-stage closed-versus-open delta. Conditions are separate FILES rather
than phases of one run because the `log` verb belongs to genet-probe and never
reaches Turnstone, so in-run phase markers would have meant changing another
repo to instrument a receipt.

Each scenario states the length of its measured block in its header comment and
the harness's case table repeats it; they must move together. The harness reads
the trailing block and refuses a run whose measured frames are not in the
palette state the condition claims, so a scenario that silently stopped opening
the palette fails loudly instead of quietly comparing the control against
itself.

Still not covered: **input-to-present latency**. The frame log times the frame,
not the wait from keystroke to photons. That needs its own instrument.

## 2026-08-23 fix

Chrome now holds one `RetainedSubtreeLayout` per window-root. An unchanged
chrome paints from the retained cascade; it re-cascades when its DOM changes,
its window resizes, or the appearance sheet is replaced. The hit-test path
shares the same retained layout, so a pointer press no longer builds its own
cascade to answer where it landed.

One thing about the shape is worth keeping, because getting it wrong would have
been silent. `RetainedLayout`, the pane twin, drains the DOM's mutation queue
inside its own `ensure`. Copying that per root would have been a stale-window
bug rather than a slow one: the chrome is N window-roots over ONE `ScriptedDom`
and therefore one mutation queue, so whichever root rendered first would have
swallowed the batch and left every other window painting a tree that no longer
existed. `RetainedSubtreeLayout` deliberately does not drain;
`ChromeSurfaces::absorb_dom_mutations` drains once and invalidates every root.
`one_mutation_invalidates_every_window_root` asserts it, with the primary
rendering first because that is the ordering that would hide the bug.

Same receipt, same machine, same window, three runs per condition:

| Condition | frame_ms before | frame_ms after | chrome_scene_ms before | chrome_scene_ms after |
| --- | --- | --- | --- | --- |
| palette closed, idle | 10.162 | 6.057 | 4.608 | 0.093 |
| palette open, idle | 44.427 | 7.235 | 36.509 | 0.204 |
| palette open, row selection | 43.483 | 8.051 | 35.474 | 0.234 |
| palette open, text edits | 42.392 | 5.802 | 35.665 | 0.120 |

The whole-frame ratio falls from **4.37x to 1.19x**, and the `chrome_scene_ms`
delta from **31.901 ms to 0.110 ms**. The closed case improved as much in
proportion, which is what the reframe predicted: the fix was never about the
palette.

Raster fell without being touched: idle dirty tiles dropped from 2151 to 1073
closed and from 2835 to 1255 open. A retained cascade produces stable
fragments, so fewer tiles differ frame to frame.

`chrome_scene_ms` p95 under row selection is 3.596 ms against a 0.234 ms
median. That is correct rather than a leak: moving the selection changes a
row's class, which is a real DOM mutation and a real re-cascade. Retention is
meant to skip the frames where nothing changed, not the frames where something
did.

The frame is now dominated by `pane_scenes_ms` at about 3.8 ms in every
condition, unchanged by this work and untouched by it. That is the next site if
frame cost is worth pursuing further; it is not part of this note.

## Done condition

Leaving the palette open causes no repeated state, layout, or raster work by
itself. Row selection and text editing update only the chrome work their
changed state requires. The responsible stage has an executable
closed-versus-open regression receipt, and the instrumentation remains
available for later chrome features.

**Met, clause by clause.** Repeated *state* work: `suggestion_runs` is 0 across
every measured open-idle frame, and was before this work began. Repeated
*layout* work: `chrome_scene_ms` is about 0.2 ms open idle, and the unit guard
holds `layout_rebuilds` at 1 across idle frames. Repeated *raster* work: chrome
repaints **zero** tiles per settled frame, closed or open — see the split
below. Row selection and editing updating only what changed: visible in both
the p95 and the split. The receipt and the instrumentation: in the tree.

## 2026-08-24 per-surface raster split

The fix left one clause unresolved and honestly so: idle frames still repainted
about 3 tiles each, and `dirty_tiles` summed across every surface, so the note
could not say whether that was chrome leaking or the graph pane working.
Guessing was the wrong move at that point; splitting the counter was the cheap
one. `dirty_tiles` is now recorded per surface class, with a count of surfaces
rasterized beside it so that a class reading zero tiles is distinguishable from
a class absent from the frame.

Summed across 360 measured idle frames per condition:

| Condition | class | rasterized | dirty tiles | per frame |
| --- | --- | --- | --- | --- |
| palette closed, idle | graph | 360 | 1334 | 3.71 |
| palette closed, idle | chrome | 360 | **0** | **0.00** |
| palette open, idle | graph | 360 | 1334 | 3.71 |
| palette open, idle | chrome | 360 | **0** | **0.00** |
| palette open, row selection | chrome | 150 | 5760 | 38.40 |
| palette open, text edits | chrome | 135 | 96 | 0.71 |

Chrome rasterizes every frame — the non-zero `rasterized` says it is present,
not skipped — and repaints nothing. The residual belongs entirely to the graph
surface, and the graph's count is **identical to the tile** with the palette
open and closed, which is the strongest available statement that the palette
adds no raster work at all. The earlier note guessed the graph pane was the
likelier owner; the split confirms it rather than leaving it a guess.

Row selection's 38.40 tiles a frame is the selection highlight moving, which is
a real visual change. It is the shape retention is supposed to leave behind:
nothing repaints on the frames where nothing changed, and the band that moved
repaints on the frames where it did.

**A methodological note, because the headline number stopped working.** This
run reports a closed-versus-open whole-frame ratio of 0.74x — an open palette
apparently rendering *faster* than a closed one. It is not faster. The chrome
delta is now approximately 0.06 ms, while `pane_scenes_ms` varies by around
1.5 ms between runs of the identical scenario, so the ratio is measuring the
graph surface's own run-to-run variance and nothing else. The ratio was the
right headline at 4.37x and is meaningless at 1.19x or below. Read the stage
fields and the per-class split; the ratio has done its job and should not be
quoted further.

## Open, and not this note's

Why does the graph surface repaint about 3.7 tiles on a settled frame with no
input, no animation asked for, and an offline graph? The count is deterministic
across runs and identical across conditions, so it is reproducible and cheap to
chase. It has nothing to do with the palette, the chrome, or this note; it is
recorded here only because this instrument is what made it visible.

## Stop line

This note does not redesign the command palette, change the resident topology,
or establish a numeric budget before the release baseline exists. The baseline
above is one machine, one window size, and one scene; it is enough to name the
stage, and not enough to set a budget.
