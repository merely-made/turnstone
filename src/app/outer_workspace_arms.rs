// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::App;
use crate::{
    action::{Effect, SpaceRef},
    panes::{PaneContent, PaneId},
    surface::FocusTarget,
};
use workbench::{TileEvent, WorkbenchEffect, WorkspaceEvent};

impl App {
    pub(super) fn outer_workspace_event(
        &mut self,
        space: SpaceRef,
        event: WorkspaceEvent,
    ) -> Vec<Effect> {
        if let Err(reason) = self.ensure_shared_space(space) {
            self.events
                .push(crate::observe::AppEvent::InteractionMissed {
                    what: "outer-workspace",
                    target: reason,
                });
            return vec![Effect::Redraw];
        }
        let id = match &event {
            WorkspaceEvent::Tile(
                TileEvent::Activated(id)
                | TileEvent::Closed(id)
                | TileEvent::Dragged { tile: id, .. },
            ) => Some(PaneId(id.0)),
            WorkspaceEvent::Float(event) => Some(PaneId(match event {
                workbench::FloatEvent::Float { tile, .. }
                | workbench::FloatEvent::Dock { tile, .. }
                | workbench::FloatEvent::SetRect { tile, .. }
                | workbench::FloatEvent::SetConstraints { tile, .. }
                | workbench::FloatEvent::SetPinned { tile, .. }
                | workbench::FloatEvent::SetVisible { tile, .. }
                | workbench::FloatEvent::Raise(tile) => tile.0,
            })),
            _ => None,
        };
        if let Some(id) = id {
            if self.space_of(id) != Some(space) {
                return vec![Effect::Redraw];
            }
        }
        if let WorkspaceEvent::Tile(TileEvent::Closed(id)) = &event {
            if self
                .space(space)
                .is_some_and(|layout| layout.iter_leaves().count() <= 1)
            {
                self.events
                    .push(crate::observe::AppEvent::InteractionMissed {
                        what: "close-pane",
                        target: "The window must keep one pane".into(),
                    });
                return vec![Effect::Redraw];
            }
            let prior = self.active_pane;
            let prior_focus = self.focus;
            self.active_pane = Some(PaneId(id.0));
            let effects = self.close_active_pane();
            if self.space_of(PaneId(id.0)).is_none() {
                if let Some(prior) = prior.filter(|pane| self.space_of(*pane).is_some()) {
                    self.active_pane = Some(prior);
                    self.focus = prior_focus;
                } else {
                    let next = self
                        .space(space)
                        .and_then(|layout| {
                            layout
                                .outer_workspace()
                                .ok()
                                .and_then(|frame| frame.active_ids().first().copied())
                        })
                        .map(|tile| PaneId(tile.0));
                    self.active_pane = next;
                    if let Some(pane) = next {
                        self.focus = if matches!(self.pane_content(pane), Some(PaneContent::Orrery))
                        {
                            FocusTarget::Graph(pane)
                        } else {
                            FocusTarget::Pane(pane)
                        };
                    }
                }
            }
            self.index_pane_spaces();
            return effects;
        }
        let outcome = self
            .space_mut(space)
            .and_then(|layout| layout.apply_workspace(&event).ok());
        if let Some(outcome) = outcome {
            if let Some(WorkbenchEffect::TearOut { tile }) = outcome.effect() {
                self.active_pane = Some(PaneId(tile.0));
                return self.tear_out_active_pane();
            }
            if let Some(id) = id {
                self.active_pane = Some(id);
                self.focus = if matches!(self.pane_content(id), Some(PaneContent::Orrery)) {
                    FocusTarget::Graph(id)
                } else {
                    FocusTarget::Pane(id)
                };
            }
            self.index_pane_spaces();
            if outcome.changed() {
                return if matches!(
                    event,
                    WorkspaceEvent::Tile(TileEvent::DividerMoved { .. })
                        | WorkspaceEvent::Float(workbench::FloatEvent::SetRect { .. })
                ) {
                    vec![Effect::Redraw]
                } else {
                    vec![Effect::SaveSession, Effect::Redraw]
                };
            }
        }
        vec![Effect::Redraw]
    }
}
