// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(in super::super) fn set_layout_strategy(
        &mut self,
        id: Option<&'static str>,
    ) -> Vec<Effect> {
        if let Some(pane) = self.scene_command_pane() {
            if let Some(graph) = self.graph_for_pane(pane) {
                self.graph_runtimes.get_mut(graph).unwrap().scene_transition = None;
            }
            self.with_graph_pane(pane, |canvas| {
                canvas.take_stop_return();
                canvas.pick_layout_strategy(id.map(str::to_string));
                canvas.set_projection_score(None);
                canvas.set_strategy_regions(Vec::new());
            });
            self.mark_graph_scene_changed(pane);
        }
        vec![Effect::Redraw]
    }

    pub(in super::super) fn toggle_size_by_recency(&mut self) -> Vec<Effect> {
        let on = !self.graph_runtimes.size_by_recency();
        self.graph_runtimes.set_size_by_recency(on);
        // A size change moves extents and the recency ordering, so the
        // active analytic layout must recompute; re-selecting the same
        // strategy drops its input cache (last_strategy_inputs = None).
        let active = self.graph_runtimes.layout_strategy().map(str::to_string);
        self.graph_runtimes.set_layout_strategy(active);
        vec![Effect::Redraw]
    }

    pub(in super::super) fn set_node_sprite(
        &mut self,
        member: Uuid,
        data_uri: String,
        hull: Vec<(f32, f32)>,
    ) -> Vec<Effect> {
        self.graph_runtimes.set_node_sprite(member, data_uri);
        // The traced collider: the node collides at its picture. Under
        // 3 points the tracer found no opaque region — keep the
        // silhouette collider rather than installing a degenerate one.
        if hull.len() >= 3 {
            self.graph_runtimes.set_node_sprite_hull(member, hull);
        }
        self.events.push(AppEvent::NodeSpriteSet(member));
        vec![Effect::SaveSession, Effect::Redraw]
    }
}
