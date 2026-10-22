// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The physics controls that compose rather than pick: role, mix, grouping,
//! schedule, single law and speed. Each carries out the same
//! `mere::canvas::physics_controls` call the web host's form makes and keeps
//! its status line on the graph runtime for the Arrange pane to read.

use super::*;
use mere::canvas::physics_controls::{self, SPEED_PRESETS};
use mere::canvas::{PhysicsChoice, PhysicsKindSource, Role};

impl App {
    pub(super) fn compose_graph_physics(&mut self, action: Action) -> Vec<Effect> {
        let Some(pane) = self.scene_command_pane() else {
            return vec![];
        };
        let outcome = self.with_graph_pane(pane, |canvas| match action {
            Action::SetPhysicsRole(id) => Role::parse(id)
                .ok_or_else(|| format!("unknown role {id}"))
                .and_then(|role| physics_controls::apply_role(canvas, role)),
            Action::SetPhysicsRoleGroups(id) => PhysicsKindSource::parse(id)
                .ok_or_else(|| format!("unknown groups source {id}"))
                .and_then(|source| physics_controls::apply_role_group_source(canvas, source)),
            Action::ApplyPhysicsMix(mix) => physics_controls::apply_mix(canvas, &mix),
            Action::ApplyPhysicsGrouping(grouping) => {
                physics_controls::apply_grouping(canvas, &grouping)
            },
            Action::ApplyPhysicsSchedule(schedule) => {
                physics_controls::apply_two_stage_schedule(canvas, &schedule)
            },
            Action::UseSingleLaw => {
                let choice = PhysicsChoice::live(canvas);
                physics_controls::apply_single_law(canvas, &choice)
            },
            Action::SetPhysicsSpeed(id) => {
                debug_assert!(SPEED_PRESETS.iter().any(|(known, _)| *known == id));
                physics_controls::apply_speed(canvas, id)
            },
            _ => Err("not a composing physics action".into()),
        });
        let Some(outcome) = outcome else {
            return vec![];
        };
        if let Some(runtime) = self
            .graph_for_pane(pane)
            .and_then(|graph| self.graph_runtimes.get_mut(graph))
        {
            runtime.physics_status = Some(outcome.unwrap_or_else(|error| format!("Refused · {error}")));
        }
        self.mark_graph_scene_changed(pane);
        vec![Effect::Redraw]
    }
}
