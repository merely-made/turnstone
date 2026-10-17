// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The retained per-pane renderers, and the one place that owns their lifetime.
//!
//! Each Cambium pane keeps a `GenetAppRunner` whose shared DOM, widget state, and
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
//! keeps its runner, actual mounted subtree and scroll across the move -- which is
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
    /// One forest document; each native window lays out its own subtree.
    pub(crate) dom: cambium::DomHandle,
    pub(crate) native_images: super::native_composition::Images,
    pub(crate) native_masks: super::native_composition::Masks,
    pub(crate) placeholder: HashMap<PaneId, crate::inline_pane::InlinePane>,
    /// Product-contributed retained sessions. This is the only dynamic map;
    /// concrete product state stays erased behind the shared surface contract.
    pub(crate) contributed: crate::contributed_surface::ContributedSurfaceSessions,
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
    /// The application-settings projection over the host provider.
    pub(crate) settings: HashMap<PaneId, crate::settings_pane::SettingsPane>,
    /// The Arrange pane: the active canvas's arrangement and physics, as controls.
    pub(crate) arrange: HashMap<PaneId, crate::arrange_pane::ArrangePane>,
    /// Owner controls for the retained Knot publishing service.
    pub(crate) publish: HashMap<PaneId, crate::publish_pane::PublishPane>,
    /// Recipient controls for a private ticket.
    pub(crate) shared_knot: HashMap<PaneId, crate::share_reader_pane::SharedKnotPane>,
    /// This device's resident receipt cards, read through the first-party door.
    pub(crate) device_receipts: HashMap<PaneId, crate::device_receipts_pane::DeviceReceiptsPane>,
    /// The disclosed scene as navigable semantics, with satisfaction chrome.
    pub(crate) frozen_projection:
        HashMap<PaneId, crate::frozen_projection_pane::FrozenProjectionPane>,
    /// The Shell Transcript pane: the ledger's visible projection.
    pub(crate) transcript: HashMap<PaneId, crate::transcript_pane::TranscriptPane>,
    /// The Overmap pane (O1): the switcher as a graph view.
    pub(crate) overmap: HashMap<PaneId, crate::swatch_pane::SwatchPane>,
}

impl PaneRenderers {
    /// Refresh host leaf paint and scroll from the measured shared fragments.
    /// State/data sync already ran before mounting, so this does not rebuild owners.
    pub(crate) fn refresh_inline_paint(&mut self, pane: PaneId, width: u32, height: u32) {
        macro_rules! find { ($($map:ident),+) => { $(if let Some(view)=self.$map.get_mut(&pane) { view.scene(width, height); return; })+ }; }
        find!(
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
    }
    pub(crate) fn blur_inline(&mut self, pane: PaneId) {
        macro_rules! find { ($($map:ident),+) => { $(if let Some(view)=self.$map.get_mut(&pane) { view.blur_inline(); return; })+ }; }
        find!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
    }
    pub(crate) fn inline_accepts_text(&self, pane: PaneId) -> bool {
        macro_rules! find { ($($map:ident),+) => { $(if let Some(view)=self.$map.get(&pane) { return view.inline_accepts_text(); })+ }; }
        find!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
        false
    }
    pub(crate) fn inline_cursor_area(&self, pane: PaneId) -> Option<(f32, f32, f32, f32)> {
        macro_rules! find { ($($map:ident),+) => { $(if let Some(view)=self.$map.get(&pane) { return view.inline_cursor_area(); })+ }; }
        find!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
        None
    }
    pub(crate) fn inline_focus(&self, pane: PaneId) -> Option<genet_scripted_dom::NodeId> {
        macro_rules! find { ($($map:ident),+) => { $(
            if let Some(view) = self.$map.get(&pane) { return view.inline_focus(); }
        )+ }; }
        find!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
        None
    }
    pub(crate) fn focus_inline(&mut self, pane: PaneId, node: genet_scripted_dom::NodeId) -> bool {
        macro_rules! find { ($($map:ident),+) => { $(
            if let Some(view) = self.$map.get_mut(&pane) { return view.focus_inline(node); }
        )+ }; }
        find!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
        false
    }
    pub(crate) fn inline_selector(
        &self,
        pane: PaneId,
        selector: &taproot::Selector,
    ) -> Result<Option<(f32, f32)>, &'static str> {
        macro_rules! find { ($($map:ident),+) => { $(
            if let Some(view) = self.$map.get(&pane) { return view.inline_selector(selector); }
        )+ }; }
        find!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
        Ok(None)
    }
    pub(crate) fn owner_roots(&self) -> Vec<genet_scripted_dom::NodeId> {
        let mut roots = Vec::new();
        macro_rules! roots { ($($map:ident),+) => { $(
            roots.extend(self.$map.values().filter_map(|view| view.inline_mount().map(|m| m.node())));
        )+ }; }
        roots!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
        roots
    }
    pub(crate) fn inline_mount(&self, pane: PaneId) -> Option<cambium::RetainedMount> {
        macro_rules! find { ($($map:ident),+) => { $(
            if let Some(view) = self.$map.get(&pane) {
                if let Some(mount) = view.inline_mount() { return Some(mount); }
            }
        )+ }; }
        find!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
        None
    }
    pub(crate) fn bind_inline(&mut self, pane: PaneId, layout: &crate::ui::InlineLayoutHandle) {
        macro_rules! bind { ($($map:ident),+) => { $(
            if let Some(view) = self.$map.get_mut(&pane) { view.bind_inline(layout); return; }
        )+ }; }
        bind!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
    }
    pub(crate) fn unbind_inline(&mut self, pane: PaneId) {
        macro_rules! unbind { ($($map:ident),+) => { $(
            if let Some(view) = self.$map.get_mut(&pane) { view.unbind_inline(); return; }
        )+ }; }
        unbind!(
            placeholder,
            roster,
            gloss,
            overmap,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            transcript
        );
    }
    /// Drop every renderer belonging to `pane`.
    ///
    /// Called when a pane closes and when a lens window tears down. The
    /// runners are `!Send` and retained for the pane's lifetime, so instance
    /// keying without eviction would leak on every close -- a leak the old
    /// one-per-kind shape could not have had.
    pub(crate) fn evict(&mut self, pane: PaneId) {
        let Self {
            dom: _,
            native_images: _,
            native_masks: _,
            placeholder,
            contributed,
            roster,
            gloss,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            device_receipts,
            frozen_projection,
            overmap,
            transcript,
        } = self;
        // Destructured on purpose: adding an eleventh map makes this fail to
        // compile until it is named here, which a list of `self.x.remove(..)`
        // lines would not.
        contributed.remove(pane);
        placeholder.remove(&pane);
        roster.remove(&pane);
        gloss.remove(&pane);
        trail.remove(&pane);
        inspector.remove(&pane);
        workbench.remove(&pane);
        settings.remove(&pane);
        arrange.remove(&pane);
        publish.remove(&pane);
        shared_knot.remove(&pane);
        device_receipts.remove(&pane);
        frozen_projection.remove(&pane);
        overmap.remove(&pane);
        transcript.remove(&pane);
    }

    /// Deliver a wheel delta to whichever renderer holds `pane`.
    ///
    /// Returns whether anything took it, so the caller only redraws when it
    /// did. Every list renderer is named here; a pane kind added without an
    /// arm simply will not scroll, which is why the fan-out is explicit rather
    /// than a default. The swatch-backed panes (Gloss, Overmap) are absent on
    /// purpose: they are custom-paint canvases whose wheel gesture is a pan,
    /// owned by the canvas rather than by a scroll container.
    pub(crate) fn scroll_pane(&mut self, pane: PaneId, dx: f32, dy: f32) -> bool {
        macro_rules! deliver {
            ($($map:ident),+ $(,)?) => {
                $(if let Some(view) = self.$map.get_mut(&pane) {
                    view.scroll_by(dx, dy);
                    return true;
                })+
            };
        }
        deliver!(
            device_receipts,
            frozen_projection,
            roster,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            transcript,
        );
        false
    }

    /// Whether any pane still has a visible overlay bar, so the shell knows to
    /// keep drawing the fade out rather than stopping on the last scrolled
    /// frame.
    pub(crate) fn any_bars_visible(&mut self) -> bool {
        if self.contributed.any_bars_visible() {
            return true;
        }
        macro_rules! visible {
            ($($map:ident),+ $(,)?) => {
                $(if self.$map.values_mut().any(|view| view.bars_visible()) {
                    return true;
                })+
            };
        }
        visible!(
            device_receipts,
            frozen_projection,
            roster,
            trail,
            inspector,
            workbench,
            settings,
            arrange,
            publish,
            shared_knot,
            transcript,
        );
        false
    }

    /// Fade activity belongs only to panes admitted in this window's plan.
    pub(crate) fn bars_visible_in(&mut self, surfaces: &[crate::surface::Surface]) -> bool {
        surfaces.iter().any(|surface| {
            let crate::surface::SurfaceKind::Pane(pane) = surface.kind else {
                return false;
            };
            if let Some(view) = self.contributed.get_mut(pane) {
                return view.bars_visible();
            }
            macro_rules! visible { ($($map:ident),+ $(,)?) => { $(
                if let Some(view) = self.$map.get_mut(&pane) { return view.bars_visible(); }
            )+ }; }
            visible!(
                device_receipts,
                frozen_projection,
                roster,
                trail,
                inspector,
                workbench,
                settings,
                arrange,
                publish,
                shared_knot,
                transcript
            );
            false
        })
    }

    /// How many renderers are retained across every kind. Test seam: it is how
    /// a receipt observes retention and eviction without reaching into fields.
    #[cfg(test)]
    pub(crate) fn retained(&self) -> usize {
        self.contributed.len()
            + self.placeholder.len()
            + self.roster.len()
            + self.gloss.len()
            + self.trail.len()
            + self.inspector.len()
            + self.workbench.len()
            + self.settings.len()
            + self.publish.len()
            + self.shared_knot.len()
            + self.device_receipts.len()
            + self.frozen_projection.len()
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

    #[test]
    fn scrollbar_continuation_is_scoped_to_actual_window_pane_plan() {
        let mut renderers = PaneRenderers::default();
        let scrolled = pane(7);
        let quiet = pane(8);
        renderers.roster.insert(scrolled, RosterGrid::new());
        renderers.roster.insert(quiet, RosterGrid::new());
        assert!(renderers.scroll_pane(scrolled, 0.0, 40.0));
        let surface = |pane| crate::surface::Surface {
            id: crate::surface::SurfaceId::pane(pane),
            kind: crate::surface::SurfaceKind::Pane(pane),
            rect: crate::surface::Rect::new(0.0, 0.0, 512.0, 480.0),
        };
        assert!(
            !renderers.bars_visible_in(&[surface(quiet)]),
            "an unrelated quiet lens owns no fade"
        );
        assert!(
            renderers.bars_visible_in(&[surface(scrolled)]),
            "the scrolled receiving lens keeps painting its fade"
        );
        assert!(
            !renderers.bars_visible_in(&[]),
            "closed or withdrawn pane has no window admission"
        );
        renderers.evict(scrolled);
        assert!(!renderers.bars_visible_in(&[surface(scrolled)]));
    }

    /// Click a Roster grid's tab by label, through the same resolver the
    /// scenario runner aims with. Driving the public surface rather than
    /// reaching into the runner is deliberate: it is the surface the shell
    /// itself uses, so a receipt that passes here cannot be passing through a
    /// door the shell does not have.
    fn click_tab(grid: &mut RosterGrid, label: &str) {
        let _ = grid.scene(W, H);
        let (x, y) = grid
            .resolve(&taproot::Selector::class("tab").containing(label), RECT)
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
