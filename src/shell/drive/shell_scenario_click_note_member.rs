// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn scenario_click_note_member(&mut self) -> Result<(), String> {
        let pane = self.app.graph_command_pane().ok_or("no graph owner pane")?;
        let (member, point) = self
            .app
            .with_graph_pane(pane, |canvas| {
                let matches: Vec<_> = canvas
                    .graph()
                    .nodes()
                    .filter(|(_, node)| node.url().starts_with("mere://redshank/note/"))
                    .map(|(key, node)| (key, node.id))
                    .collect();
                let [(key, member)] = matches.as_slice() else {
                    return Err("note member missing or ambiguous");
                };
                let point = canvas
                    .screen_position_of(*key)
                    .ok_or("note member is not painted")?;
                Ok((*member, point))
            })
            .ok_or("graph absent")??;
        let scale = self.app.pane_layout_scale(pane);
        let surface = self
            .surface_plan()
            .into_iter()
            .find(|s| s.kind == crate::surface::SurfaceKind::Graph(pane))
            .ok_or("graph surface is not visible")?;
        let (x, y) = (
            surface.rect.x + point.0 * scale,
            surface.rect.y + point.1 * scale,
        );
        if !surface.rect.contains(x, y) {
            return Err("note member outside graph surface".into());
        }
        self.deliver_move(x, y);
        self.deliver_press(x, y, winit::event::MouseButton::Left);
        self.deliver_release(x, y, winit::event::MouseButton::Left);
        let selected = self
            .app
            .with_graph_pane(pane, |canvas| canvas.selected_members())
            .ok_or("graph absent after pointer")?;
        if selected.as_slice() != [member] {
            return Err("pointer did not select the unique note".into());
        }
        self.request_redraw();
        Ok(())
    }
}
