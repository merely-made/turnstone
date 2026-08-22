# Command Palette Open-Lag Note

**Date:** 2026-08-22
**Status:** Observed, not yet measured or attributed.
**Scope:** Turnstone chrome and frame production.

## Observation

Interaction lags while the command palette is open. The symptom is tied to the
open state, not merely the opening transition.

Do not assign this to Knot, Graphshell, or the device resident from proximity.
Those systems may be excluded or implicated by profiling, but the first owner
is Turnstone's chrome path.

## Current hot-path candidates

These are code-reading candidates, not findings:

1. Every palette edit calls `recompute_omnibar_suggestions` before requesting a
   redraw.
2. On every rendered chrome surface, `ChromeView::sync` reconstructs the row
   views, updates every window projection, and `ChromeView::scene` calls
   `scene_from_subtree`.
3. The pane path has `RetainedLayout`; the chrome subtree path does not. Its own
   module comment still says the palette document rebuilds wholesale per state
   change.
4. The shell already emits whole-frame `frame_ms`, but it does not yet separate
   suggestion computation, chrome update, cascade/layout/paint, rasterization,
   and composition.

## Receipt

Use one release build, profile, window size, and settled scene. Record three
runs each with the palette closed and open. During each run, measure idle
redraws, row-selection keys, and text edits. Publish:

- median and p95 whole-frame time;
- input-to-present latency;
- chrome update, cascade/layout/paint, raster, and composition time;
- suggestion count and whether the resident/network lanes were enabled.

Repeat once with resident and network activity disabled. That classifies the
boundary; it is not the primary benchmark.

## Done condition

Leaving the palette open causes no repeated state, layout, or raster work by
itself. Row selection and text editing update only the chrome work their changed
state requires. The responsible stage has an executable closed-versus-open
regression receipt, and the instrumentation remains available for later chrome
features.

## Stop line

This note does not redesign the command palette, change the resident topology,
or establish a numeric budget before the release baseline exists.
