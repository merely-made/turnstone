// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! One shared authority for tiled and floating pane stations in every window.
use super::App;
use crate::{
    action::{Effect, SpaceRef},
    observe::AppEvent,
    panes::{PaneId, PaneNode},
};
use workbench::{Edge, FloatDockTarget, FloatEvent, RelativeRect, TileId};

impl App {
    pub(crate) fn prepare_shared_stations_for_save(&mut self) -> Result<(), String> {
        // Validate every window before publishing any migrated authority.
        let primary = self.frisket.clone();
        let lenses = self.lenses.clone();
        let primary_blueprint = self.primary_blueprint.clone();
        let lens_blueprints = self.lens_blueprints.clone();
        let spaces = std::iter::once(SpaceRef::Primary)
            .chain(
                self.lenses
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| s.as_ref().map(|_| SpaceRef::Lens(i))),
            )
            .collect::<Vec<_>>();
        for space in spaces {
            if let Err(reason) = self.ensure_shared_space(space) {
                self.frisket = primary;
                self.lenses = lenses;
                self.primary_blueprint = primary_blueprint;
                self.lens_blueprints = lens_blueprints;
                return Err(reason);
            }
        }
        Ok(())
    }
    pub(crate) fn ensure_shared_space(&mut self, space: SpaceRef) -> Result<(), String> {
        let layout = self.space(space).ok_or("Pane window is closed")?;
        let mut frame = layout.outer_workspace()?.into_owned();
        if self.blueprint_space(space).is_none() && matches!(layout.root, PaneNode::Workspace(_)) {
            crate::panes::workspace_bridge::validate_descriptors(&frame)?;
            return Ok(());
        }
        if let Some(blueprint) = self.blueprint_space(space) {
            frame = crate::panes::workspace_blueprint::import(blueprint, &frame)?;
        }
        // Validate and decode all descriptors before retiring the former authority.
        crate::panes::workspace_bridge::validate_descriptors(&frame)?;
        self.space_mut(space).ok_or("Pane window is closed")?.root =
            PaneNode::Workspace(Box::new(frame));
        self.take_blueprint_space(space);
        Ok(())
    }

    fn station_refusal(&mut self, reason: String) -> Vec<Effect> {
        self.events.push(AppEvent::InteractionMissed {
            what: "pane-station",
            target: reason,
        });
        vec![Effect::Redraw]
    }

    pub(super) fn float_active_pane(&mut self) -> Vec<Effect> {
        let Some((pane, space)) = self
            .active_pane
            .and_then(|p| self.space_of(p).map(|s| (p, s)))
        else {
            return vec![Effect::Redraw];
        };
        if let Err(reason) = self.ensure_shared_space(space) {
            return self.station_refusal(reason);
        }
        let frame = self.space_mut(space).unwrap().workspace_mut().unwrap();
        let rect = if frame.workspace.floating().len() % 2 == 0 {
            RelativeRect {
                x: 0.54,
                y: 0.12,
                width: 0.42,
                height: 0.48,
            }
        } else {
            RelativeRect {
                x: 0.08,
                y: 0.16,
                width: 0.42,
                height: 0.48,
            }
        };
        if !frame
            .apply(
                &FloatEvent::Float {
                    tile: TileId(pane.0),
                    rect,
                }
                .into(),
            )
            .is_ok_and(|o| o.changed())
        {
            return vec![Effect::Redraw];
        }
        self.maximized = None;
        if let Some(content) = self.pane_content(pane) {
            self.events
                .push(AppEvent::PaneFloated(content.tag().into()));
        }
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn dock_active_pane(&mut self) -> Vec<Effect> {
        let Some((pane, space)) = self
            .active_pane
            .and_then(|p| self.space_of(p).map(|s| (p, s)))
        else {
            return vec![Effect::Redraw];
        };
        if let Err(reason) = self.ensure_shared_space(space) {
            return self.station_refusal(reason);
        }
        let frame = self.space_mut(space).unwrap().workspace_mut().unwrap();
        let target = frame
            .workspace
            .tiled()
            .tiles()
            .first()
            .map(|t| FloatDockTarget::Beside {
                target: t.id,
                edge: Edge::Right,
            })
            .unwrap_or(FloatDockTarget::TiledRoot);
        if !frame
            .apply(
                &FloatEvent::Dock {
                    tile: TileId(pane.0),
                    target,
                }
                .into(),
            )
            .is_ok_and(|o| o.changed())
        {
            return vec![Effect::Redraw];
        }
        if let Some(content) = self.pane_content(pane) {
            self.events.push(AppEvent::PaneDocked(content.tag().into()));
        }
        vec![Effect::SaveSession, Effect::Redraw]
    }

    fn transfer_shared_pane(
        &mut self,
        source: SpaceRef,
        destination: SpaceRef,
        pane: PaneId,
    ) -> Result<(), String> {
        if source == destination {
            return Err("Pane already belongs to destination".into());
        }
        self.ensure_shared_space(source)?;
        self.ensure_shared_space(destination)?;
        let mut from = self.space(source).unwrap().outer_workspace()?.into_owned();
        let mut to = self
            .space(destination)
            .unwrap()
            .outer_workspace()?
            .into_owned();
        if from.panes.len() <= 1 {
            return Err("The window must keep one pane".into());
        }
        let target = to.workspace.tiled().tiles().last().map(|t| t.id.0);
        from.transfer_to(&mut to, pane.0, target)?;
        self.space_mut(source).unwrap().root = PaneNode::Workspace(Box::new(from));
        self.space_mut(destination).unwrap().root = PaneNode::Workspace(Box::new(to));
        if self.maximized == Some(pane) {
            self.maximized = None;
        }
        self.active_pane = Some(pane);
        self.index_pane_spaces();
        Ok(())
    }

    pub(super) fn return_active_pane_to_primary(&mut self) -> Vec<Effect> {
        let Some((pane, source @ SpaceRef::Lens(_))) = self
            .active_pane
            .and_then(|p| self.space_of(p).map(|s| (p, s)))
        else {
            return vec![Effect::Redraw];
        };
        if let Err(reason) = self.ensure_shared_space(source) {
            return self.station_refusal(reason);
        }
        if !self
            .space(source)
            .unwrap()
            .outer_workspace()
            .unwrap()
            .workspace
            .floating()
            .iter()
            .any(|f| f.tile.id.0 == pane.0)
        {
            return vec![Effect::Redraw];
        }
        if let Err(reason) = self.transfer_shared_pane(source, SpaceRef::Primary, pane) {
            return self.station_refusal(reason);
        }
        if let Some(content) = self.pane_content(pane) {
            self.events
                .push(AppEvent::PaneReturned(content.tag().into()));
        }
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn tear_out_active_pane(&mut self) -> Vec<Effect> {
        let Some((pane, source)) = self
            .active_pane
            .and_then(|p| self.space_of(p).map(|s| (p, s)))
        else {
            return vec![Effect::Redraw];
        };
        if let Err(reason) = self.ensure_shared_space(source) {
            return self.station_refusal(reason);
        }
        if self.space(source).unwrap().iter_leaves().count() <= 1 {
            return self.station_refusal("The window must keep one pane".into());
        }
        let (ordinal, mut effects) = self.target_lens(Some(source));
        if let Err(reason) = self.transfer_shared_pane(source, SpaceRef::Lens(ordinal), pane) {
            return self.station_refusal(reason);
        }
        if let Some(content) = self.pane_content(pane) {
            self.events
                .push(AppEvent::PaneTornOut(content.tag().into()));
        }
        effects.extend([Effect::SaveSession, Effect::Redraw]);
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        action::Action,
        panes::{PaneKindId, PaneSource, SerializedSource, SourceRef, SourceSchemaId},
    };
    #[test]
    fn provider_source_stays_with_one_pane_through_float_window_return_and_save() {
        let mut app = App::test_stub();
        let source = PaneSource::Fixed(SourceRef::External {
            schema: SourceSchemaId::new("test.document"),
            payload: SerializedSource {
                version: 1,
                payload: serde_json::json!({"document":"same book","access":"this checkout"}),
            },
        });
        app.update(Action::SummonContributedPane {
            kind: PaneKindId::new("test.document"),
            source: source.clone(),
        });
        let pane = app.active_pane.unwrap();
        app.update(Action::FloatActivePane);
        app.update(Action::TearOutActivePane);
        assert_eq!(app.space_of(pane), Some(SpaceRef::Lens(0)));
        assert!(app.primary_blueprint.is_none());
        assert!(app.lens_blueprints.iter().all(Option::is_none));
        app.update(Action::ReturnActivePaneToPrimary);
        app.update(Action::DockActivePane);
        let reopened: crate::panes::FrisketLayout =
            serde_json::from_str(&serde_json::to_string(&app.frisket).unwrap()).unwrap();
        let frame = reopened.outer_workspace().unwrap();
        let spec: crate::panes::PaneSpec =
            serde_json::from_value(frame.descriptors[&pane.0].clone()).unwrap();
        assert_eq!(spec.source, source);
        assert_eq!(spec.id, pane);
        assert!(frame.workspace.floating().is_empty());
        assert!(
            app.lenses[0]
                .as_ref()
                .unwrap()
                .iter_leaves()
                .all(|(id, _, _)| id != pane)
        );
    }
    #[test]
    fn last_pane_tearout_does_not_create_a_window_or_orphan_a_float() {
        let mut app = App::test_stub();
        app.active_pane = Some(app.default_graph_pane());
        app.update(Action::FloatActivePane);
        let before = app.frisket.clone();
        assert_eq!(app.update(Action::TearOutActivePane), vec![Effect::Redraw]);
        assert_eq!(app.frisket, before);
        assert!(app.lenses.is_empty());
    }
    #[test]
    fn compatible_legacy_stations_import_once_and_invalid_blueprints_refuse_without_mutation() {
        let mut app = App::test_stub();
        app.update(Action::SummonPane(PaneKindId::new(
            crate::panes::kind::ROSTER,
        )));
        let pane = app.active_pane.unwrap();
        let mut blueprint = crate::panes::blueprint_from_frisket(&app.frisket);
        assert!(blueprint.float_pane(
            pane,
            crate::panes::RelativeRect {
                x: 0.2,
                y: 0.3,
                width: 0.4,
                height: 0.5
            }
        ));
        app.primary_blueprint = Some(blueprint);
        app.ensure_shared_space(SpaceRef::Primary).unwrap();
        assert!(app.primary_blueprint.is_none());
        assert_eq!(
            app.frisket.outer_workspace().unwrap().workspace.floating()[0]
                .tile
                .id
                .0,
            pane.0
        );
        let mut invalid = crate::panes::blueprint_from_frisket(&app.frisket);
        invalid.panes.clear();
        app.primary_blueprint = Some(invalid.clone());
        let prior = app.frisket.clone();
        assert!(app.ensure_shared_space(SpaceRef::Primary).is_err());
        assert_eq!(app.frisket, prior);
        assert_eq!(app.primary_blueprint, Some(invalid));
    }
    #[test]
    fn failed_multiwindow_save_preflight_preserves_every_authority() {
        let mut app = App::test_stub();
        app.update(Action::NewWindow);
        app.primary_blueprint = Some(crate::panes::blueprint_from_frisket(&app.frisket));
        let mut invalid = crate::panes::blueprint_from_frisket(app.lenses[0].as_ref().unwrap());
        invalid.panes.clear();
        app.lens_blueprints[0] = Some(invalid);
        let before = (
            app.frisket.clone(),
            app.lenses.clone(),
            app.primary_blueprint.clone(),
            app.lens_blueprints.clone(),
        );
        assert!(app.prepare_shared_stations_for_save().is_err());
        assert_eq!(
            (
                app.frisket,
                app.lenses,
                app.primary_blueprint,
                app.lens_blueprints
            ),
            before
        );
    }
}
