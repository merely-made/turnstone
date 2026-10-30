// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

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
    let _ = grid.scene(W, H, 1.0);
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
