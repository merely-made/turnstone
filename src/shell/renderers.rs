//! The retained per-pane renderers, and the one place that owns their lifetime.
//!
//! Each Cambium pane keeps a `GenetAppRunner` whose DOM, widget state, and
//! scroll persist between the frame that draws it and the click that hits it.
//! They are `!Send`, so they live in the shell rather than in `App`.
//!
//! ## Why these are keyed by `PaneId`, and why that is the whole point
//!
//! Before A1 the shell held one runner per *kind* (`roster_grid:
//! Option<RosterGrid>`). Summoning minted a fresh `PaneId` every time, so two
//! Rosters were two leaves sharing one runtime: one selection, one scroll, one
//! set of controls between them.
//!
//! Keying by `PaneId` fixes that without giving up the property the old shape
//! had by accident. A pane torn out to a lens window keeps its `PaneId`, so it
//! keeps its runner, so it keeps its DOM and scroll across the move -- which is
//! what makes tear-out identity-preserving. Sharing a runner across one pane's
//! stations is the feature; sharing it across two panes was the bug.
//!
//! ## Why they live in one struct
//!
//! [`PaneRenderers::evict`] must clear *every* map, and a hand-written list of
//! `remove` calls in the shell is one map behind the moment someone adds an
//! eleventh pane. Collecting them here means eviction is written once beside
//! the fields it clears, and the receipt below can prove the routing without
//! constructing a `Shell` -- which needs a winit event loop and four actors.

use std::collections::HashMap;

use crate::panes::PaneId;

/// Every retained pane renderer, keyed by the pane instance that owns it.
#[derive(Default)]
pub(crate) struct PaneRenderers {
    /// The Roster pane's cambium grid (rung 5 slice D).
    pub(crate) roster: HashMap<PaneId, crate::cambium_pane::RosterGrid>,
    /// The Gloss pane (minimap): carries a custom-paint leaf, so it owns a
    /// leaf registry beside its runner.
    pub(crate) gloss: HashMap<PaneId, crate::swatch_pane::SwatchPane>,
    /// The Trail pane: the sectioned list's first consumer.
    pub(crate) trail: HashMap<PaneId, crate::trail_pane::TrailPane>,
    /// The Inspector pane: detail sections over app truth.
    pub(crate) inspector: HashMap<PaneId, crate::inspector_pane::InspectorPane>,
    /// The Workbench pane (rung 5 slice E): platen's tiling in cambium strips.
    pub(crate) workbench: HashMap<PaneId, crate::workbench_pane::WorkbenchPane>,
    /// The Apparatus pane: the focused node's viewer override.
    pub(crate) apparatus: HashMap<PaneId, crate::apparatus_pane::ApparatusPane>,
    /// The application-settings projection over the host provider.
    pub(crate) settings: HashMap<PaneId, crate::settings_pane::SettingsPane>,
    /// Owner controls for the retained Knot publishing service.
    pub(crate) publish: HashMap<PaneId, crate::publish_pane::PublishPane>,
    /// Recipient controls for a private ticket.
    pub(crate) shared_knot: HashMap<PaneId, crate::share_reader_pane::SharedKnotPane>,
    /// This device's resident receipt cards, read through the first-party door.
    pub(crate) device_receipts: HashMap<PaneId, crate::device_receipts_pane::DeviceReceiptsPane>,
    /// The Shell Transcript pane: the ledger's visible projection.
    pub(crate) transcript: HashMap<PaneId, crate::transcript_pane::TranscriptPane>,
    /// The Overmap pane (O1): the switcher as a graph view.
    pub(crate) overmap: HashMap<PaneId, crate::swatch_pane::SwatchPane>,
}

impl PaneRenderers {
    /// Drop every renderer belonging to `pane`.
    ///
    /// Called when a pane closes and when a lens window tears down. The
    /// runners are `!Send` and retained for the pane's lifetime, so instance
    /// keying without eviction would leak on every close -- a leak the old
    /// one-per-kind shape could not have had.
    pub(crate) fn evict(&mut self, pane: PaneId) {
        let Self {
            roster,
            gloss,
            trail,
            inspector,
            workbench,
            apparatus,
            settings,
            publish,
            shared_knot,
            device_receipts,
            overmap,
            transcript,
        } = self;
        // Destructured on purpose: adding an eleventh map makes this fail to
        // compile until it is named here, which a list of `self.x.remove(..)`
        // lines would not.
        roster.remove(&pane);
        gloss.remove(&pane);
        trail.remove(&pane);
        inspector.remove(&pane);
        workbench.remove(&pane);
        apparatus.remove(&pane);
        settings.remove(&pane);
        publish.remove(&pane);
        shared_knot.remove(&pane);
        device_receipts.remove(&pane);
        overmap.remove(&pane);
        transcript.remove(&pane);
    }

    /// How many renderers are retained across every kind. Test seam: it is how
    /// a receipt observes retention and eviction without reaching into fields.
    #[cfg(test)]
    pub(crate) fn retained(&self) -> usize {
        self.roster.len()
            + self.gloss.len()
            + self.trail.len()
            + self.inspector.len()
            + self.workbench.len()
            + self.apparatus.len()
            + self.settings.len()
            + self.publish.len()
            + self.shared_knot.len()
            + self.device_receipts.len()
            + self.overmap.len()
            + self.transcript.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cambium_pane::RosterGrid;

    /// The pane rect every receipt below lays out and clicks in.
    const RECT: [f32; 4] = [0.0, 0.0, 512.0, 600.0];
    const W: u32 = 512;
    const H: u32 = 600;

    fn pane(n: u64) -> PaneId {
        PaneId(n)
    }

    /// Click a Roster grid's tab by label, through the same resolver the
    /// scenario runner aims with. Driving the public surface rather than
    /// reaching into the runner is deliberate: it is the surface the shell
    /// itself uses, so a receipt that passes here cannot be passing through a
    /// door the shell does not have.
    fn click_tab(grid: &mut RosterGrid, label: &str) {
        let (x, y) = grid
            .resolve(&genet_probe::Selector::class("tab").containing(label), RECT)
            .unwrap_or_else(|| panic!("the strip must draw a {label} tab"));
        grid.click(x, y, W, H);
    }

    /// A1's done-condition, at the layer where the defect actually was.
    ///
    /// The pre-existing `two_roster_runners_keep_independent_selection` builds
    /// two `RosterGrid`s directly and shows they differ -- which is a property
    /// of them being two values, and would have passed against the one-runner-
    /// per-kind shell this lane replaced. The claim that needed proving is
    /// about the shell's *routing*: two `PaneId`s resolve to two retained
    /// runners.
    #[test]
    fn two_panes_of_one_kind_resolve_to_two_retained_runners() {
        let mut renderers = PaneRenderers::default();
        let (first, second) = (pane(1), pane(2));

        // The routing the shell performs each frame, verbatim from render.rs:
        // entry(pane_id).or_insert_with(RosterGrid::new).
        renderers
            .roster
            .entry(first)
            .or_insert_with(RosterGrid::new);
        renderers
            .roster
            .entry(second)
            .or_insert_with(RosterGrid::new);
        assert_eq!(renderers.retained(), 2, "one runner per pane instance");

        // Drive one pane's tab strip. The other must not follow.
        click_tab(
            renderers.roster.get_mut(&first).expect("the first pane"),
            "Links",
        );

        assert_eq!(
            renderers.roster[&first].selected_tab(),
            (1, "Links"),
            "the pane that was clicked moved"
        );
        assert_eq!(
            renderers.roster[&second].selected_tab(),
            (0, "Nodes"),
            "the pane that was not clicked is untouched"
        );
    }

    /// Re-resolving an existing pane returns the same runner rather than a
    /// fresh one. This is what makes a pane's selection and scroll survive the
    /// frame that drew it, and what keeps tear-out identity-preserving: the
    /// `PaneId` travels, so the runner does.
    #[test]
    fn resolving_the_same_pane_twice_keeps_its_state() {
        let mut renderers = PaneRenderers::default();
        let id = pane(7);

        renderers.roster.entry(id).or_insert_with(RosterGrid::new);
        click_tab(renderers.roster.get_mut(&id).expect("retained"), "Links");

        // A later frame resolves the same id.
        renderers.roster.entry(id).or_insert_with(RosterGrid::new);

        assert_eq!(renderers.retained(), 1, "no second runner was minted");
        assert_eq!(
            renderers.roster[&id].selected_tab(),
            (1, "Links"),
            "the retained selection survived re-resolution"
        );
    }

    /// Eviction drops exactly one pane's renderers, across every kind, and
    /// leaves its neighbours alone.
    #[test]
    fn eviction_takes_one_pane_and_spares_the_rest() {
        let mut renderers = PaneRenderers::default();
        let (doomed, keeper) = (pane(1), pane(2));

        // The doomed pane holds renderers of two different kinds, so a partial
        // eviction that cleared only one map would show up here.
        renderers
            .roster
            .entry(doomed)
            .or_insert_with(RosterGrid::new);
        renderers
            .trail
            .entry(doomed)
            .or_insert_with(crate::trail_pane::TrailPane::new);
        renderers
            .roster
            .entry(keeper)
            .or_insert_with(RosterGrid::new);
        assert_eq!(renderers.retained(), 3);

        renderers.evict(doomed);

        assert_eq!(renderers.retained(), 1, "both of the doomed pane's went");
        assert!(!renderers.roster.contains_key(&doomed));
        assert!(!renderers.trail.contains_key(&doomed));
        assert!(
            renderers.roster.contains_key(&keeper),
            "a neighbour's runner is not collateral"
        );
    }

    /// Evicting a pane that holds nothing is a no-op rather than a panic. The
    /// shell evicts on every close, including panes whose renderer was never
    /// built because they were never drawn.
    #[test]
    fn evicting_an_undrawn_pane_is_harmless() {
        let mut renderers = PaneRenderers::default();
        renderers
            .roster
            .entry(pane(1))
            .or_insert_with(RosterGrid::new);
        renderers.evict(pane(99));
        assert_eq!(renderers.retained(), 1);
    }
}
