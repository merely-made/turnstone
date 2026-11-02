// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ArrangeIntent {
    /// The palette-equivalent action, or `None` for an id the catalogs do
    /// not know (a stale row, a typo in a script).
    pub fn into_action(self) -> Option<Action> {
        match self {
            Self::Verb(verb) => Action::from_canvas_verb(verb),
            Self::SetRole(id) => field_option(FieldId::Role, &id).map(Action::SetPhysicsRole),
            Self::SetRoleGroups(id) => {
                field_option(FieldId::RoleGroups, &id).map(Action::SetPhysicsRoleGroups)
            },
            Self::SetSpeed(id) => field_option(FieldId::Speed, &id).map(Action::SetPhysicsSpeed),
            Self::ApplyMix(mix) => Some(Action::ApplyPhysicsMix(mix)),
            Self::ApplyGrouping(grouping) => Some(Action::ApplyPhysicsGrouping(grouping)),
            Self::ApplySchedule(schedule) => Some(Action::ApplyPhysicsSchedule(schedule)),
            Self::UseSingleLaw => Some(Action::UseSingleLaw),
            ArrangeIntent::SetLayout(id) if id == FREE_LAYOUT_ID => {
                Some(Action::SetLayoutStrategy(None))
            },
            ArrangeIntent::SetLayout(id) => CANVAS_LAYOUT_STRATEGIES
                .iter()
                .find(|(known, _)| *known == id)
                .map(|(known, _)| Action::SetLayoutStrategy(Some(known))),
            ArrangeIntent::SetLaw(id) => {
                field_option(FieldId::PhysicsLaw, &id).map(Action::SetPhysicsLaw)
            },
            ArrangeIntent::SetOverlay { id, on } => PhysicsOverlay::ALL
                .iter()
                .find(|overlay| overlay.id() == id)
                .map(|overlay| Action::SetPhysicsOverlay(overlay.id(), on)),
            ArrangeIntent::SetKindSource(id) => {
                field_option(FieldId::PhysicsKind, &id).map(Action::SetPhysicsKindSource)
            },
            ArrangeIntent::SetMassSource(id) => {
                field_option(FieldId::PhysicsMass, &id).map(Action::SetPhysicsMassSource)
            },
            ArrangeIntent::SetDepthSource(id) => {
                field_option(FieldId::PhysicsDepth, &id).map(Action::SetPhysicsDepthSource)
            },
            ArrangeIntent::ApplyProfile(id) => field_option(FieldId::PhysicsProfile, &id)
                .filter(|profile| *profile != "custom")
                .map(Action::ApplyPhysicsProfile),
        }
    }
}
