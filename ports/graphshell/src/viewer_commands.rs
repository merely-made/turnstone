// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The retained viewer's supported commands, independent of a browser/store.
use command_menu::{Command, CommandSet, catalogue::ids};

use crate::{canvas_controls::CanvasCommand, canvas_gestures::WheelMode};

#[derive(Clone, Copy, Debug, Default)]
pub struct Availability {
    pub hosted: bool,
    pub selection: bool,
    pub locked: bool,
    pub paused: bool,
    /// None when the host has no optional history surface.
    pub history: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Canvas(CanvasCommand),
    Wheel(WheelMode),
    Tools,
    History,
}

/// A small retained menu page keeps every command reachable without relying
/// on touch scrolling in the embedding browser host. Keyboard selection drives
/// the same page, and callers preserve the full catalogue for search.
pub fn browsing_page(selected: Option<usize>, count: usize) -> std::ops::Range<usize> {
    let start = selected.unwrap_or(0).min(count.saturating_sub(1)) / 2 * 2;
    start..(start + 2).min(count)
}

pub fn action(id: &str, hosted: bool) -> Option<Action> {
    Some(match id {
        ids::VIEW_FIT => Action::Canvas(if hosted {
            CanvasCommand::FitVisible
        } else {
            CanvasCommand::Fit
        }),
        "fit-selection" => Action::Canvas(CanvasCommand::FitSelection),
        "restore-arrangement" => Action::Canvas(CanvasCommand::RestoreArrangement),
        ids::PHYSICS_TOGGLE => Action::Canvas(CanvasCommand::TogglePhysics),
        ids::VIEW_ZOOM_IN => Action::Canvas(CanvasCommand::Zoom { delta: 40.0 }),
        ids::VIEW_ZOOM_OUT => Action::Canvas(CanvasCommand::Zoom { delta: -40.0 }),
        "pan-left" => Action::Canvas(CanvasCommand::Pan { dx: -42.0, dy: 0.0 }),
        "pan-right" => Action::Canvas(CanvasCommand::Pan { dx: 42.0, dy: 0.0 }),
        "pan-up" => Action::Canvas(CanvasCommand::Pan { dx: 0.0, dy: -42.0 }),
        "pan-down" => Action::Canvas(CanvasCommand::Pan { dx: 0.0, dy: 42.0 }),
        "wheel-zoom" => Action::Wheel(WheelMode::Zoom),
        "wheel-pan" => Action::Wheel(WheelMode::Pan),
        "wheel-page" => Action::Wheel(WheelMode::Page),
        "graph-tools" => Action::Tools,
        "view-history" => Action::History,
        _ => return None,
    })
}

pub fn commands(available: Availability) -> CommandSet {
    let mut set = CommandSet::new().with_defaults([
        ids::VIEW_FIT,
        "fit-selection",
        "restore-arrangement",
        ids::PHYSICS_TOGGLE,
        "graph-tools",
    ]);
    for (id, label, category) in [
        (
            ids::VIEW_FIT,
            if available.hosted {
                "Fit visible graph"
            } else {
                "Fit graph"
            },
            "canvas",
        ),
        ("fit-selection", "Fit selection", "node"),
        ("restore-arrangement", "Restore arrangement", "canvas"),
        (
            ids::PHYSICS_TOGGLE,
            if available.paused {
                "Play physics"
            } else {
                "Pause physics"
            },
            "canvas",
        ),
        (ids::VIEW_ZOOM_IN, "Zoom in", "canvas"),
        (ids::VIEW_ZOOM_OUT, "Zoom out", "canvas"),
        ("pan-left", "Pan left", "canvas"),
        ("pan-right", "Pan right", "canvas"),
        ("pan-up", "Pan up", "canvas"),
        ("pan-down", "Pan down", "canvas"),
        ("graph-tools", "Graph tools", "canvas"),
        ("wheel-zoom", "Wheel: zoom graph", "preferences"),
        ("wheel-pan", "Wheel: pan graph", "preferences"),
        ("wheel-page", "Wheel: scroll page", "preferences"),
    ] {
        set.register(Command::new(id, label, category));
        if available.locked && matches!(action(id, available.hosted), Some(Action::Canvas(_))) {
            set.set_disabled(id, Some("Finish the pending graph operation first".into()));
        }
    }
    if let Some(open) = available.history {
        set.register(Command::new(
            "view-history",
            if open { "Hide history" } else { "Show history" },
            "canvas",
        ));
    }
    if !available.selection && !available.locked {
        set.set_disabled("fit-selection", Some("Select a visible node first".into()));
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use command_menu::{CommandChoices, MenuSession};

    #[test]
    fn browsing_pages_cover_commands_and_follow_keyboard_selection() {
        for count in 0..40 {
            let mut covered = Vec::new();
            for first in (0..count).step_by(2) {
                let page = browsing_page(Some(first), count);
                assert!(page.len() <= 2);
                covered.extend(page);
            }
            assert_eq!(covered, (0..count).collect::<Vec<_>>());
            for selected in 0..count {
                assert!(browsing_page(Some(selected), count).contains(&selected));
            }
        }
        assert_eq!(browsing_page(None, 0), 0..0);
        assert_eq!(browsing_page(Some(99), 3), 2..3);
    }

    #[test]
    fn history_command_is_only_offered_by_a_host_with_history() {
        assert!(
            commands(Availability::default())
                .get("view-history")
                .is_none()
        );
        let closed = commands(Availability {
            history: Some(false),
            ..Default::default()
        });
        assert_eq!(closed.get("view-history").unwrap().label, "Show history");
        let open = commands(Availability {
            history: Some(true),
            ..Default::default()
        });
        assert_eq!(open.get("view-history").unwrap().label, "Hide history");
        assert_eq!(action("view-history", true), Some(Action::History));
    }

    #[test]
    fn hosted_fit_never_turns_into_a_placement_restore() {
        assert_eq!(
            action(ids::VIEW_FIT, true),
            Some(Action::Canvas(CanvasCommand::FitVisible))
        );
        assert_eq!(
            action(ids::VIEW_FIT, false),
            Some(Action::Canvas(CanvasCommand::Fit))
        );
        assert_eq!(
            action("restore-arrangement", true),
            Some(Action::Canvas(CanvasCommand::RestoreArrangement))
        );
        assert!(action("save-scene", true).is_none());
    }

    #[test]
    fn unavailable_selection_and_locked_operations_remain_explained() {
        let unavailable = commands(Availability::default());
        assert!(
            unavailable
                .get("fit-selection")
                .unwrap()
                .disabled_reason
                .is_some()
        );
        let locked = commands(Availability {
            selection: true,
            locked: true,
            ..Default::default()
        });
        for id in [
            ids::VIEW_FIT,
            "fit-selection",
            "pan-left",
            ids::PHYSICS_TOGGLE,
        ] {
            assert!(locked.get(id).unwrap().disabled_reason.is_some());
        }
        assert!(locked.get("wheel-page").unwrap().disabled_reason.is_none());
        assert!(
            commands(Availability {
                selection: true,
                ..Default::default()
            })
            .get("fit-selection")
            .unwrap()
            .disabled_reason
            .is_none()
        );
    }

    #[test]
    fn shared_search_keeps_context_first_and_finds_nondefault_preferences() {
        let set = commands(Availability {
            selection: true,
            ..Default::default()
        });
        let mut session = MenuSession::open(Some("node"));
        let choices = CommandChoices::default();
        assert_eq!(session.rows(&set, &choices)[0].id, "fit-selection");
        session.set_query("scroll page");
        assert_eq!(
            session.chosen(&session.rows(&set, &choices)).unwrap().id,
            "wheel-page"
        );
    }
}
