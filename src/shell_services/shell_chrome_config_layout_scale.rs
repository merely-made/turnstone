// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ShellChromeConfig {
    /// The layout scale of window `slot`; windows not yet observed are 1.
    pub fn layout_scale(&self, slot: usize) -> f32 {
        crate::ui::clean_scale(self.layout_scales.get(&slot).copied().unwrap_or(1.0))
    }

    /// Apply durable composition from a space blueprint while retaining
    /// application-owned row, shortcut, and retention policy.
    pub fn apply_blueprint(&mut self, blueprint: &ChromeBlueprint) {
        self.omnibar.placement = blueprint.omnibar.clone();
        self.shellbar.placement = blueprint.shellbar.clone();
        self.shellbar.visible = !matches!(blueprint.shellbar, ChromePlacement::Hidden);
        self.transcript_placement = blueprint.transcript.clone();
    }

    /// Replace the shellbar's visible edge without making settings code reach
    /// into a renderer.
    pub fn set_shellbar_edge(&mut self, edge: ChromeEdge) {
        self.shellbar.placement = ChromePlacement::Docked(edge);
    }

    pub fn projects_omnibar(&self) -> bool {
        !matches!(
            self.omnibar.placement,
            ChromePlacement::Hidden | ChromePlacement::Pane(_)
        )
    }

    pub fn projects_shellbar(&self) -> bool {
        self.shellbar.visible
            && !matches!(
                self.shellbar.placement,
                ChromePlacement::Hidden | ChromePlacement::Pane(_)
            )
    }
}
