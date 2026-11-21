// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use scenograph::presentation::{
    Context, Effect, InputAvailability, PrimitiveKind, Property, Scalar, ViewState,
};
use scenomise::projection::ProjectionValue;

pub(super) fn override_scenes(
    page: &TreePage,
    p: &ProjectionControls,
    subject: &Subject,
) -> Option<Vec<ContextScene>> {
    if !page.presentation.live_enabled() {
        return None;
    }
    let (rules, _) = page.presentation.saved_rules()?;
    let kind = match subject.kind {
        SubjectKind::Node | SubjectKind::Selection => PrimitiveKind::Node,
        SubjectKind::Relation => PrimitiveKind::Relation,
        SubjectKind::Field => PrimitiveKind::Field,
    };
    if !rules.rules.iter().any(|rule| {
        rule.enabled
            && rule.target.kind == kind
            && (rule.target.ids.is_empty() || rule.target.ids.contains(&subject.id))
            && rule
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::Contexts { .. }))
    }) {
        return None;
    }
    let mut context = Context {
        id: subject.id.clone(),
        kind,
        facts: Default::default(),
        states: Default::default(),
        screen_size: None,
        previous_matches: Default::default(),
    };
    let mut available = InputAvailability::default();
    match kind {
        PrimitiveKind::Node => {
            let row = p
                .dataset
                .as_ref()?
                .occurrences
                .iter()
                .find(|row| row.occurrence_id == subject.id)?;
            context.facts = row
                .values
                .iter()
                .filter_map(|(name, value)| {
                    let value = match value {
                        ProjectionValue::Text(text) => Scalar::Text(text.clone()),
                        ProjectionValue::Number(number) => Scalar::Number(*number),
                        ProjectionValue::Boolean(value) => Scalar::Bool(*value),
                        ProjectionValue::Reference(_)
                        | ProjectionValue::Date(_)
                        | ProjectionValue::DateTime(_)
                        | ProjectionValue::List(_) => return None,
                    };
                    Some((name.clone(), value))
                })
                .collect();
            available.states.insert(ViewState::Selected);
            if p.selected.as_deref() == Some(&subject.id) {
                context.states.insert(ViewState::Selected);
            }
        },
        PrimitiveKind::Relation => {
            let edge = p.relationships.iter().find(|edge| edge.id == subject.id)?;
            context
                .facts
                .insert("kind".into(), Scalar::Text(edge.kind.clone()));
            context
                .facts
                .insert("label".into(), Scalar::Text(edge.label.clone()));
        },
        PrimitiveKind::Field => {
            p.dataset.as_ref()?.fields.get(&subject.id)?;
        },
    }
    let winner = rules
        .resolve_with_availability(&context, &available)
        .ok()?
        .winners
        .remove(&Property::Contexts)?;
    let Effect::Contexts { value } = winner.effect else {
        return None;
    };
    Some(value)
}
