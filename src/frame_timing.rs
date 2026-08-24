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
    /// Raster work split by surface class. Totals are derived from it rather
    /// than tracked beside it, so the parts cannot drift from the whole.
    pub surface_raster: SurfaceRasterSplit,
    /// Composing the layers onto the acquired frame and presenting it.
    pub compose: Duration,
}

/// Raster accounting for one surface class.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRaster {
    /// Scenes of this class rasterized in the frame. Imported
    /// external-texture layers are not counted; their own producer painted
    /// them.
    pub rasterized: u32,
    /// Tiles the rasterizer actually rebuilt across them, from netrender's
    /// per-surface dirty count.
    pub dirty_tiles: usize,
}

impl SurfaceRaster {
    fn note(&mut self, dirty: usize) {
        self.rasterized += 1;
        self.dirty_tiles += dirty;
    }

    fn add(&mut self, other: Self) {
        self.rasterized += other.rasterized;
        self.dirty_tiles += other.dirty_tiles;
    }
}

/// Raster work split by surface class.
///
/// One summed `dirty_tiles` answers "did anything repaint" and then refuses to
/// say WHAT, which is exactly where this instrument's first pass stopped: a
/// settled frame repainting three tiles could be chrome leaking or the graph
/// pane animating, and the total cannot tell them apart. That ambiguity was
/// recorded as an open clause of the palette open-lag note's done condition
/// rather than guessed at; this is the split that closes it.
///
/// `rasterized` rides beside `dirty_tiles` in each class because a class
/// reading zero dirty tiles is otherwise indistinguishable from a class that
/// was not in the frame at all — "chrome is clean" and "there is no chrome"
/// are different findings.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRasterSplit {
    pub graph: SurfaceRaster,
    pub content: SurfaceRaster,
    pub pane: SurfaceRaster,
    pub divider: SurfaceRaster,
    pub chrome: SurfaceRaster,
}

impl SurfaceRasterSplit {
    /// Record one rasterized surface. Keyed off [`SurfaceKind`] rather than
    /// its label so a new surface class fails to compile here instead of
    /// quietly landing in no bucket.
    pub fn note(&mut self, kind: crate::surface::SurfaceKind, dirty: usize) {
        self.bucket(kind).note(dirty);
    }

    fn bucket(&mut self, kind: crate::surface::SurfaceKind) -> &mut SurfaceRaster {
        use crate::surface::SurfaceKind;
        match kind {
            SurfaceKind::Graph(_) => &mut self.graph,
            SurfaceKind::Content(_) => &mut self.content,
            SurfaceKind::Pane(_) => &mut self.pane,
            SurfaceKind::Divider(_) => &mut self.divider,
            SurfaceKind::Chrome => &mut self.chrome,
        }
    }

    /// The whole, derived from the parts.
    pub fn totals(&self) -> SurfaceRaster {
        let mut total = SurfaceRaster::default();
        for class in [
            self.graph,
            self.content,
            self.pane,
            self.divider,
            self.chrome,
        ] {
            total.add(class);
        }
        total
    }

    fn add(&mut self, other: Self) {
        self.graph.add(other.graph);
        self.content.add(other.content);
        self.pane.add(other.pane);
        self.divider.add(other.divider);
        self.chrome.add(other.chrome);
    }
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

    /// Fold one rasterization pass's per-class counts into the frame.
    pub fn note_surface_raster(&mut self, split: SurfaceRasterSplit) {
        self.surface_raster.add(split);
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
        timings
            .surface_raster
            .note(crate::surface::SurfaceKind::Chrome, 234);

        timings.reset();

        // A keystroke's cost belongs to the frame that presented it, not to
        // every frame after.
        assert_eq!(timings, FrameTimings::default());
    }

    #[test]
    fn raster_totals_are_derived_from_the_split() {
        use crate::panes::PaneId;
        use crate::surface::SurfaceKind;


        let mut split = SurfaceRasterSplit::default();
        split.note(SurfaceKind::Chrome, 3);
        split.note(SurfaceKind::Graph(PaneId(1)), 40);
        split.note(SurfaceKind::Graph(PaneId(1)), 2);

        // The question the summed counter could not answer: chrome rendered
        // and was nearly clean, the graph did the repainting.
        assert_eq!(split.chrome.rasterized, 1);
        assert_eq!(split.chrome.dirty_tiles, 3);
        assert_eq!(split.graph.rasterized, 2);
        assert_eq!(split.graph.dirty_tiles, 42);

        // A class that was not in the frame reads zero rasterized, which is
        // what distinguishes it from a class that was clean.
        assert_eq!(split.pane, SurfaceRaster::default());

        let totals = split.totals();
        assert_eq!(totals.rasterized, 3);
        assert_eq!(totals.dirty_tiles, 45);
    }

    #[test]
    fn ms_reports_the_frame_log_unit() {
        assert!((ms(Duration::from_micros(1500)) - 1.5).abs() < f32::EPSILON);
    }
}
