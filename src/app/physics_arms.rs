// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The physics controls that compose rather than pick: role, mix, grouping,
//! schedule, single law and speed. Each carries out the same
//! `mere::canvas::physics_form::FormAction` the web host's form makes and keeps
//! its status line on the graph runtime for the Arrange pane to read.

use super::*;
use mere::canvas::physics_controls::SPEED_PRESETS;
use mere::canvas::physics_form::FormAction;
use mere::canvas::{PhysicsChoice, PhysicsKindSource, Role};

impl App {
    pub(super) fn compose_graph_physics(&mut self, action: Action) -> Vec<Effect> {
        let Some(pane) = self.scene_command_pane() else {
            return vec![];
        };
        let outcome = self.with_graph_pane(pane, |canvas| {
            let request = match action {
                Action::SetPhysicsRole(id) => Role::parse(id)
                    .map(FormAction::Role)
                    .ok_or_else(|| format!("unknown role {id}")),
                Action::SetPhysicsRoleGroups(id) => PhysicsKindSource::parse(id)
                    .map(FormAction::RoleGroups)
                    .ok_or_else(|| format!("unknown groups source {id}")),
                Action::ApplyPhysicsMix(mix) => Ok(FormAction::Mix(mix)),
                Action::ApplyPhysicsGrouping(grouping) => Ok(FormAction::Grouping(grouping)),
                Action::ApplyPhysicsSchedule(schedule) => Ok(FormAction::Schedule(schedule)),
                Action::UseSingleLaw => Ok(FormAction::SingleLaw(PhysicsChoice::live(canvas))),
                Action::SetPhysicsSpeed(id) => {
                    debug_assert!(SPEED_PRESETS.iter().any(|(known, _)| *known == id));
                    Ok(FormAction::Speed(id))
                },
                _ => Err("not a composing physics action".into()),
            };
            match request {
                Ok(request) => request.run(canvas).status,
                Err(error) => format!("Refused · {error}"),
            }
        });
        let Some(status) = outcome else {
            return vec![];
        };
        if let Some(runtime) = self
            .graph_for_pane(pane)
            .and_then(|graph| self.graph_runtimes.get_mut(graph))
        {
            runtime.physics_status = Some(status);
        }
        self.mark_graph_scene_changed(pane);
        vec![Effect::Redraw]
    }
}
