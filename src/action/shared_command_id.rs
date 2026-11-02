// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The stack's shared command id for an action Turnstone offers under one
/// (Scenograph editor plan SE49, SE50). The palette registers these rows under
/// the id and labels them from the catalogue, so a label revision (SE51)
/// reaches Turnstone without code. Every other row keeps its label as its
/// host-owned id.
pub fn shared_command_id(action: &Action) -> Option<&'static str> {
    use cambium::catalogue::ids;
    Some(match action {
        Action::NavBack => ids::NAV_BACK,
        Action::NavForward => ids::NAV_FORWARD,
        Action::Reload => ids::NAV_RELOAD,
        Action::Stop => ids::NAV_STOP,
        Action::FitView => ids::VIEW_FIT,
        Action::FoldSelection => ids::FOLD_SELECTION,
        Action::ExpandFold => ids::FOLD_EXPAND,
        Action::TogglePhysics => ids::PHYSICS_TOGGLE,
        Action::DeleteFocusedNode => ids::NODE_DELETE,
        Action::SaveSession => ids::SESSION_SAVE,
        Action::ResetToSeed => ids::ARRANGEMENT_RESET_TO_SEED,
        Action::SetSeed => ids::ARRANGEMENT_SET_SEED,
        Action::RestoreRecipeSeed => ids::ARRANGEMENT_RESTORE_RECIPE_SEED,
        Action::DismissRetained => ids::ARRANGEMENT_DISMISS_RETAINED,
        Action::ToggleForegroundPin => ids::NODE_FOREGROUND_PIN,
        Action::GrowTiles => ids::TILE_GROW,
        Action::ShrinkTiles => ids::TILE_SHRINK,
        Action::ToggleTerritory => ids::VIEW_TERRITORY,
        Action::SummonPane(kind) => match kind.as_str() {
            crate::panes::kind::SETTINGS => ids::PANE_SETTINGS,
            crate::panes::kind::TRAIL => ids::PANE_TRAIL,
            crate::panes::kind::WORKBENCH => ids::PANE_WORKBENCH,
            _ => return None,
        },
        _ => return None,
    })
}
