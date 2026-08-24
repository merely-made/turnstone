//! Per-frame stage accounting for the chrome path.
//!
//! `frame_ms` alone names a slow frame without naming what made it slow. The
//! command palette open-lag note
//! ([`design_docs/2026-08-22_command_palette_open_lag.md`]) reproduced roughly
//! a 9x whole-frame delta with the palette open and could attribute none of
//! it, because one number covers suggestion computation, chrome update,
//! cascade/layout/paint, rasterization, and composition at once. These spans
//! separate exactly those five stages so a closed-versus-open receipt can say
//! WHICH stage moved.
//!
//! Counts sit beside the durations deliberately. The done condition is about
//! REPEATED work, not only slow work: a palette that merely sits open must
//! provoke no rebuild at all, and a duration cannot tell one expensive sync
//! from four cheap ones. `suggestion_runs` and `suggestion_refits` answer
//! "how often", `chrome_syncs` and `rasterized` answer "how many surfaces",
//! and `dirty_tiles` answers "how much of the surface was actually repainted"
//! — a settled frame that still rebuilds every tile is the failure this
//! instrument exists to catch.
//!
//! Suggestion time accumulates BETWEEN frames. It is spent on the input edge,
//! inside `App::recompute_omnibar_suggestions`, not inside `render`, so a
//! frame-local timer would read zero on the very keystroke that provoked the
//! work. The shell logs and clears the accumulator once per frame, which
//! attributes each edit's cost to the frame that presents its result.
//!
//! The whole instrument is a handful of `Instant::now()` calls per frame and
//! stays compiled in: the note's done condition asks that it remain available
//! for later chrome features, and an instrument that must be re-added is one
//! that will not be there the next time a frame feels heavy. Only the log is
//! gated (`RUST_LOG=turnstone::shell::render=debug`, the recipe the surfaces
//! plan already documents).

use std::time::Duration;

/// One frame's stage costs, reset after the frame that reports them.
///
/// Field order follows the pipeline, so a log line reads in the order the work
/// happens: suggestions on the input edge, then chrome update, then scene
/// construction, then raster, then composition and present.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FrameTimings {
    /// Time inside `recompute_omnibar_suggestions` since the last frame.
    pub suggestions: Duration,
    /// How many times suggestions were recomputed since the last frame. More
    /// than one per presented frame means input outran the renderer, which is
    /// itself a finding: the work is per-edit, not per-frame.
    pub suggestion_runs: u32,
    /// How many of those runs computed the list a SECOND time because the
    /// fitted row limit disagreed with the configured one. Counted apart from
    /// `suggestion_runs` because it is avoidable work by construction, not
    /// load: the same catalog is ranked twice for one keystroke.
    pub suggestion_refits: u32,
    /// `ChromeView::sync`: rebuilding the row views and every window's
    /// projection from app state.
    pub chrome_sync: Duration,
    /// `ChromeView::scene`: cascade, layout, and paint of the chrome subtree
    /// into a scene.
    pub chrome_scene: Duration,
    /// How many chrome surfaces were synced this frame. One sync serves every
    /// window (the one-state contract), so a number above one means a window
    /// re-entered the branch.
    pub chrome_syncs: u32,
    /// Scene construction for every non-chrome surface (graph, pane, content).
    /// Kept apart from the chrome stages so a chrome regression cannot hide
    /// behind pane cost, and vice versa.
    pub pane_scenes: Duration,
    /// Pass 2: rasterizing each planned scene to its own texture.
    pub raster: Duration,
    /// How many scenes were rasterized. Imported external-texture layers are
    /// not counted; they were painted by their own producer.
    pub rasterized: u32,
    /// Tiles the rasterizer actually rebuilt across those scenes, summed from
    /// netrender's per-surface dirty count. Zero across a settled frame is the
    /// shape the done condition asks for.
    pub dirty_tiles: usize,
    /// Composing the layers onto the acquired frame and presenting it.
    pub compose: Duration,
}

impl FrameTimings {
    /// Note one suggestion recomputation. `refit` records that the list was
    /// ranked twice because the fitted row limit was smaller than configured.
    pub fn note_suggestions(&mut self, elapsed: Duration, refit: bool) {
        self.suggestions += elapsed;
        self.suggestion_runs += 1;
        if refit {
            self.suggestion_refits += 1;
        }
    }

    /// Clear the frame-scoped stages after they have been reported.
    ///
    /// Suggestion counters clear with the rest: they are attributed to the
    /// frame that presents their result, so carrying them forward would
    /// double-count one keystroke across every later frame.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Milliseconds as `f32`, the unit the frame log already reports.
pub fn ms(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestion_runs_and_refits_count_separately() {
        let mut timings = FrameTimings::default();
        timings.note_suggestions(Duration::from_millis(3), false);
        timings.note_suggestions(Duration::from_millis(5), true);

        assert_eq!(timings.suggestion_runs, 2);
        // Only the second run ranked the catalog twice.
        assert_eq!(timings.suggestion_refits, 1);
        assert_eq!(timings.suggestions, Duration::from_millis(8));
    }

    #[test]
    fn reset_clears_between_frame_accumulation() {
        let mut timings = FrameTimings::default();
        timings.note_suggestions(Duration::from_millis(4), true);
        timings.raster = Duration::from_millis(9);
        timings.dirty_tiles = 234;

        timings.reset();

        // A keystroke's cost belongs to the frame that presented it, not to
        // every frame after.
        assert_eq!(timings, FrameTimings::default());
    }

    #[test]
    fn ms_reports_the_frame_log_unit() {
        assert!((ms(Duration::from_micros(1500)) - 1.5).abs() < f32::EPSILON);
    }
}
