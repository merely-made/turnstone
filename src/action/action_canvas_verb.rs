// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Action {
    /// The canvas verb this action carries out: one table for the shell's
    /// arms and the Arrange pane's buttons, labelled by `CanvasCommand::label`.
    pub fn canvas_verb(&self) -> Option<CanvasCommand> {
        Some(match self {
            Self::ResetToSeed => CanvasCommand::ResetToSeed,
            Self::SetSeed => CanvasCommand::SetSeed,
            Self::RestoreRecipeSeed => CanvasCommand::RestoreRecipeSeed,
            Self::ToggleForegroundPin => CanvasCommand::ToggleForegroundPin,
            Self::DismissRetained => CanvasCommand::DismissRetained,
            Self::GrowTiles => CanvasCommand::GrowTiles,
            Self::ShrinkTiles => CanvasCommand::ShrinkTiles,
            Self::ToggleTerritory => CanvasCommand::ToggleTerritory,
            _ => return None,
        })
    }

    /// The action for a canvas verb Turnstone offers, the inverse of
    /// [`Action::canvas_verb`].
    pub fn from_canvas_verb(verb: CanvasCommand) -> Option<Self> {
        [
            Self::ResetToSeed,
            Self::SetSeed,
            Self::RestoreRecipeSeed,
            Self::ToggleForegroundPin,
            Self::DismissRetained,
            Self::GrowTiles,
            Self::ShrinkTiles,
            Self::ToggleTerritory,
        ]
        .into_iter()
        .find(|action| action.canvas_verb() == Some(verb))
    }
}
