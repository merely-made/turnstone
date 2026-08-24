# Command Palette Open-Lag Note

**Date:** 2026-08-22 (attributed 2026-08-23)
**Status:** Attributed on a release build. The cost is `ChromeView::scene`,
which re-cascades, re-lays-out and repaints the chrome subtree from scratch on
every frame. The instrument and the closed-versus-open receipt are in the tree;
the fix is not.
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
   repainted". The chrome pays a full rebuild every frame instead.

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
- [`ui::hit_test_subtree`](../src/ui.rs) builds the same full snapshot the same
  way. Every hit test against chrome pays a cascade and a layout. Unmeasured
  here, and it shares whatever fix the stage above gets.

## The instrument

`turnstone::shell::render` emits one `frame` line per presented frame carrying
the whole and its parts:

```
frame frame_ms=... surfaces=... palette=... suggestions_ms=... suggestion_runs=...
      suggestion_refits=... chrome_sync_ms=... chrome_scene_ms=... chrome_syncs=...
      pane_scenes_ms=... raster_ms=... rasterized=... dirty_tiles=... compose_ms=...
```

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

## Done condition

Unchanged, and **not yet met**: leaving the palette open causes no repeated
state, layout, or raster work by itself. Row selection and text editing update
only the chrome work their changed state requires. The responsible stage has an
executable closed-versus-open regression receipt, and the instrumentation
remains available for later chrome features.

Of those, the receipt and the instrumentation now exist. The first two
sentences describe work that has not been done: chrome still rebuilds wholesale
every frame in both states.

## Stop line

This note does not redesign the command palette, change the resident topology,
or establish a numeric budget before the release baseline exists. The baseline
above is one machine, one window size, and one scene; it is enough to name the
stage, and not enough to set a budget.
