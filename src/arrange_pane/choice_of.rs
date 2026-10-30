// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Choice {
    pub(super) fn of(app: &App) -> Self {
        Self::for_graph(
            app,
            app.scene_command_pane()
                .and_then(|pane| app.graph_for_pane(pane)),
        )
    }

    pub(super) fn for_graph(app: &App, graph: Option<crate::panes::GraphId>) -> Self {
        let canvas = graph
            .and_then(|graph| app.graph_runtimes.canvas(graph))
            .unwrap_or(&app.graph_runtimes);
        let readback = Readback::of(canvas);
        Self {
            layout: canvas
                .layout_strategy()
                .map(str::to_string)
                .unwrap_or_else(|| FREE_LAYOUT_ID.to_string()),
            readback,
            status: graph
                .and_then(|graph| app.graph_runtimes.get(graph))
                .and_then(|runtime| runtime.physics_status.clone()),
            saved_seed: canvas.has_saved_seed(),
            regions: canvas.strategy_regions().len(),
            refusal: app.physics_refusal().map(ToString::to_string),
        }
    }
}
