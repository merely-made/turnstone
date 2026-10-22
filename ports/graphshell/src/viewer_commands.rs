// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The retained viewer's supported commands, independent of a browser/store.
use hest::{
    EffectClass,
    catalogue::{self, ids},
    menu::{Command, CommandSet},
};

use mere::canvas::{controls::CanvasCommand, gestures::WheelMode};

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
        ids::ARRANGEMENT_RESET_TO_SEED => Action::Canvas(CanvasCommand::ResetToSeed),
        ids::ARRANGEMENT_SET_SEED => Action::Canvas(CanvasCommand::SetSeed),
        ids::ARRANGEMENT_RESTORE_RECIPE_SEED => Action::Canvas(CanvasCommand::RestoreRecipeSeed),
        ids::NODE_FOREGROUND_PIN => Action::Canvas(CanvasCommand::ToggleForegroundPin),
        ids::ARRANGEMENT_DISMISS_RETAINED => Action::Canvas(CanvasCommand::DismissRetained),
        ids::TILE_GROW => Action::Canvas(CanvasCommand::GrowTiles),
        ids::TILE_SHRINK => Action::Canvas(CanvasCommand::ShrinkTiles),
        ids::VIEW_TERRITORY => Action::Canvas(CanvasCommand::ToggleTerritory),
        "suggestions" => Action::Canvas(CanvasCommand::ToggleSuggestions),
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

/// The established `act <id>` canvas subset. Its fit resumes layout following;
/// the hosted menu instead uses `FitVisible`. Preserve that scenario contract.
pub fn scenario_canvas_command(id: &str) -> Option<CanvasCommand> {
    match action(id, false)? {
        Action::Canvas(CanvasCommand::FitSelection | CanvasCommand::RestoreArrangement) => None,
        Action::Canvas(command) => Some(command),
        _ => None,
    }
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
                CanvasCommand::FitVisible
            } else {
                CanvasCommand::Fit
            }
            .label(),
            "canvas",
        ),
        ("fit-selection", CanvasCommand::FitSelection.label(), "node"),
        (
            "restore-arrangement",
            CanvasCommand::RestoreArrangement.label(),
            "canvas",
        ),
        (
            ids::ARRANGEMENT_RESET_TO_SEED,
            CanvasCommand::ResetToSeed.label(),
            "canvas",
        ),
        (
            ids::ARRANGEMENT_SET_SEED,
            CanvasCommand::SetSeed.label(),
            "canvas",
        ),
        (
            ids::ARRANGEMENT_RESTORE_RECIPE_SEED,
            CanvasCommand::RestoreRecipeSeed.label(),
            "canvas",
        ),
        (
            ids::NODE_FOREGROUND_PIN,
            CanvasCommand::ToggleForegroundPin.label(),
            "node",
        ),
        (
            ids::ARRANGEMENT_DISMISS_RETAINED,
            CanvasCommand::DismissRetained.label(),
            "canvas",
        ),
        (ids::TILE_GROW, CanvasCommand::GrowTiles.label(), "node"),
        (ids::TILE_SHRINK, CanvasCommand::ShrinkTiles.label(), "node"),
        (
            ids::VIEW_TERRITORY,
            CanvasCommand::ToggleTerritory.label(),
            "canvas",
        ),
        (
            "suggestions",
            CanvasCommand::ToggleSuggestions.label(),
            "preferences",
        ),
        (
            ids::PHYSICS_TOGGLE,
            CanvasCommand::SetPhysicsPaused(!available.paused).label(),
            "canvas",
        ),
        (
            ids::VIEW_ZOOM_IN,
            CanvasCommand::Zoom { delta: 1.0 }.label(),
            "canvas",
        ),
        (
            ids::VIEW_ZOOM_OUT,
            CanvasCommand::Zoom { delta: -1.0 }.label(),
            "canvas",
        ),
        (
            "pan-left",
            CanvasCommand::Pan { dx: -1.0, dy: 0.0 }.label(),
            "canvas",
        ),
        (
            "pan-right",
            CanvasCommand::Pan { dx: 1.0, dy: 0.0 }.label(),
            "canvas",
        ),
        (
            "pan-up",
            CanvasCommand::Pan { dx: 0.0, dy: -1.0 }.label(),
            "canvas",
        ),
        (
            "pan-down",
            CanvasCommand::Pan { dx: 0.0, dy: 1.0 }.label(),
            "canvas",
        ),
        ("graph-tools", "Graph tools", "canvas"),
        ("wheel-zoom", "Wheel: zoom graph", "preferences"),
        ("wheel-pan", "Wheel: pan graph", "preferences"),
        ("wheel-page", "Wheel: scroll page", "preferences"),
    ] {
        let mut command = catalogue::command(id)
            .unwrap_or_else(|| Command::new(id, label, category, EffectClass::Curation));
        command.label = label.into();
        command.category = category.into();
        set.register(command);
        if available.locked && matches!(action(id, available.hosted), Some(Action::Canvas(_))) {
            set.set_disabled(id, Some("Finish the pending graph operation first".into()));
        }
    }
    if let Some(open) = available.history {
        set.register(Command::new(
            "view-history",
            if open { "Hide history" } else { "Show history" },
            "canvas",
            EffectClass::Curation,
        ));
    }
    if !available.selection && !available.locked {
        set.set_disabled("fit-selection", Some("Select a visible node first".into()));
        set.set_disabled(ids::NODE_FOREGROUND_PIN, Some("Select a node first".into()));
        set.set_disabled(ids::TILE_GROW, Some("Select a tile first".into()));
        set.set_disabled(ids::TILE_SHRINK, Some("Select a tile first".into()));
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use hest::menu::{CommandChoices, MenuSession};

    #[test]
    fn shared_canvas_verbs_are_labelled_by_the_canvas_and_the_catalogue_alike() {
        for (id, verb) in [
            (ids::ARRANGEMENT_RESET_TO_SEED, CanvasCommand::ResetToSeed),
            (ids::ARRANGEMENT_SET_SEED, CanvasCommand::SetSeed),
            (
                ids::ARRANGEMENT_RESTORE_RECIPE_SEED,
                CanvasCommand::RestoreRecipeSeed,
            ),
            (
                ids::ARRANGEMENT_DISMISS_RETAINED,
                CanvasCommand::DismissRetained,
            ),
            (ids::NODE_FOREGROUND_PIN, CanvasCommand::ToggleForegroundPin),
            (ids::TILE_GROW, CanvasCommand::GrowTiles),
            (ids::TILE_SHRINK, CanvasCommand::ShrinkTiles),
            (ids::VIEW_TERRITORY, CanvasCommand::ToggleTerritory),
        ] {
            assert_eq!(catalogue::label(id), Some(verb.label()), "{id}");
            assert_eq!(action(id, false), Some(Action::Canvas(verb)), "{id}");
            let set = commands(Availability::default());
            assert_eq!(
                set.get(id).map(|command| command.label.as_str()),
                Some(verb.label())
            );
        }
    }

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
        assert_eq!(
            scenario_canvas_command(ids::VIEW_FIT),
            Some(CanvasCommand::Fit)
        );
        assert!(scenario_canvas_command("fit-selection").is_none());
        assert!(scenario_canvas_command("restore-arrangement").is_none());
        assert!(scenario_canvas_command("graph-tools").is_none());
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
