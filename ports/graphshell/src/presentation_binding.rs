// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Exact held occurrences and disclosed view inputs for presentation decisions.
//! No source writes, field evaluation, URL identity or synthetic assertions.
use crate::presentation_editor::presentation::{
    Condition, Context, Effect, Issue, PrimitiveKind, Property, Resolution, RuleSet, Scalar,
    ViewState,
};
use crate::{
    access_inspection::inspect_access,
    link_inspection::{LinkTarget, inspect_link},
};
use mere::kernel::graph::{FieldDefinition, FieldExtent, FieldId, Graph};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Occurrence {
    Access {
        member: Uuid,
    },
    /// One held assertion contributing to this exact displayed access pair.
    Assertion {
        pair: LinkTarget,
        statement: String,
    },
    Numen {
        id: FieldId,
    },
    Forme {
        id: Uuid,
    },
}
impl Occurrence {
    pub fn id(&self) -> String {
        match self {
            Self::Access { member } => format!("access:{member}"),
            Self::Numen { id } => format!("numen:{}", id.as_uuid()),
            Self::Forme { id } => format!("forme:{id}"),
            // JSON escaping preserves arbitrary assertion handles without collisions.
            Self::Assertion { pair, statement } => format!(
                "assertion:{}",
                serde_json::to_string(&(LinkTarget::new(pair.first, pair.second), statement))
                    .unwrap()
            ),
        }
    }
}
/// Supplied by this view; None means unavailable, never false.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewInputs {
    pub selected: Option<bool>,
    pub position_pinned: Option<bool>,
    pub inspected: bool,
    pub screen_size: Option<f64>,
    pub hovered: Option<bool>,
    pub visible: Option<bool>,
}
/// Forme's separate arrangement owner, not a Numen scalar field.
#[derive(Clone, Debug, PartialEq)]
pub struct FormeFacts {
    pub id: Uuid,
    pub label: Option<String>,
    pub locked: bool,
    pub visible: bool,
    pub members: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BoundContext {
    pub occurrence: Occurrence,
    pub context: Context,
    pub available_states: BTreeSet<ViewState>,
    pub notes: Vec<String>,
}
fn text(facts: &mut BTreeMap<String, Scalar>, key: &str, value: impl Into<String>) {
    facts.insert(key.into(), Scalar::Text(value.into()));
}
fn state(bound: &mut BoundContext, state: ViewState, value: Option<bool>) {
    if let Some(value) = value {
        bound.available_states.insert(state);
        if value {
            bound.context.states.insert(state);
        }
    }
}
pub fn bind(
    graph: &Graph,
    occurrence: &Occurrence,
    view: &ViewInputs,
    forme: Option<&FormeFacts>,
) -> Result<BoundContext, String> {
    let kind = match occurrence {
        Occurrence::Access { .. } => PrimitiveKind::Node,
        Occurrence::Assertion { .. } => PrimitiveKind::Relation,
        _ => PrimitiveKind::Field,
    };
    let mut bound = BoundContext {
        occurrence: occurrence.clone(),
        context: Context { id: occurrence.id(), kind, facts: BTreeMap::new(), states: BTreeSet::new(), screen_size: view.screen_size, previous_matches: BTreeSet::new() },
        available_states: BTreeSet::new(),
        notes: vec!["Facts are held locally; absent metadata is not a completeness claim. Foreground pins and overview state have no adapter yet.".into()],
    };
    let facts = &mut bound.context.facts;
    match occurrence {
        Occurrence::Access { member } => {
            let inspected = inspect_access(graph, *member)?;
            let (_, node) = graph.get_node_by_id(*member).ok_or("Access departed")?;
            text(facts, "label", inspected.access.title);
            text(facts, "address", inspected.access.address);
            if let Some(mime) = &node.media_type {
                text(facts, "media_type", mime);
                let category = mime.split('/').next().unwrap_or("");
                if ["image", "video", "audio", "text"].contains(&category) {
                    text(facts, "media", category);
                }
            }
            if let Some(id) = inspected.resource_id {
                text(facts, "resource_id", id.to_string());
            }
            if let Some(resource) = inspected.resource {
                text(facts, "resource_address", resource.address);
                for fact in resource.facts {
                    if let Ok(value) = serde_json::from_value::<Scalar>(fact.value) {
                        facts.insert(format!("resource.{}", fact.key), value);
                    }
                }
            }
            state(&mut bound, ViewState::Selected, view.selected);
            state(&mut bound, ViewState::PositionPinned, view.position_pinned);
        },
        Occurrence::Assertion { pair, statement } => {
            let inspected = inspect_link(graph, LinkTarget::new(pair.first, pair.second))?;
            let (record, held) = inspected
                .records
                .iter()
                .find_map(|record| {
                    record
                        .statements
                        .iter()
                        .find(|held| held.statement_id == *statement)
                        .map(|held| (record, held))
                })
                .ok_or_else(|| {
                    format!("Assertion {statement} no longer contributes to this displayed pair")
                })?;
            text(facts, "statement_id", &held.statement_id);
            text(facts, "predicate", &held.predicate);
            text(facts, "family", "Semantic");
            text(
                facts,
                "source_stratum",
                format!("{:?}", record.owner.stratum),
            );
            text(facts, "source_from", record.owner.from.id.to_string());
            text(facts, "source_to", record.owner.to.id.to_string());
            text(facts, "displayed_from", record.displayed_from.to_string());
            text(facts, "displayed_to", record.displayed_to.to_string());
            text(
                facts,
                "scope",
                serde_json::to_string(&held.graph_scope).unwrap(),
            );
            if let Some(label) = &held.label {
                text(facts, "label", label);
            }
            if let Some(iri) = &held.provenance_iri {
                text(facts, "provenance", iri);
            }
            bound.notes.push("This is a held assertion; derived-link truth and sidecar-only relation identities are not supplied.".into());
        },
        Occurrence::Numen { id } => {
            let field = graph
                .field(*id)
                .ok_or_else(|| format!("Field {} is no longer held here", id.as_uuid()))?;
            text(facts, "role", "numen");
            text(facts, "lifecycle", format!("{:?}", field.lifecycle));
            text(
                facts,
                "definition",
                match field.definition {
                    FieldDefinition::Scalar(_) => "scalar",
                    FieldDefinition::Vector(_) => "vector",
                },
            );
            text(
                facts,
                "extent",
                match field.extent {
                    FieldExtent::Global => "global",
                    FieldExtent::Region { .. } => "region",
                    FieldExtent::AttachedToNode(_) => "attached",
                    FieldExtent::Polygon { .. } => "polygon",
                },
            );
            if let Some(name) = &field.name {
                text(facts, "label", name);
            }
            if let Some(visible) = view.visible {
                facts.insert("visible".into(), Scalar::Bool(visible));
            }
            bound.notes.push("Field definition is disclosed without evaluation; RegionGuides has no Numen renderer adapter.".into());
        },
        Occurrence::Forme { id } => {
            let forme = forme
                .filter(|forme| forme.id == *id)
                .ok_or_else(|| format!("Forme {id} is no longer presented here"))?;
            text(facts, "role", "forme");
            if let Some(label) = &forme.label {
                text(facts, "label", label);
            }
            facts.insert("locked".into(), Scalar::Bool(forme.locked));
            facts.insert("visible".into(), Scalar::Bool(forme.visible));
            facts.insert("members".into(), Scalar::Number(forme.members as f64));
            if let Some(hovered) = view.hovered {
                facts.insert("hovered".into(), Scalar::Bool(hovered));
            }
            state(&mut bound, ViewState::LayoutUnlocked, Some(!forme.locked));
            bound.notes.push("RegionGuides controls subdivisions only; outer boundary, visibility, geometry, locks and tile sessions keep their own owners.".into());
        },
    }
    state(&mut bound, ViewState::Inspected, Some(view.inspected));
    Ok(bound)
}
fn unavailable(condition: &Condition, bound: &BoundContext) -> Option<String> {
    match condition {
        Condition::State { state } if !bound.available_states.contains(state) => {
            Some(format!("view state {state:?} is unavailable"))
        },
        Condition::ScreenSize { .. } if bound.context.screen_size.is_none() => {
            Some("screen measurement is unavailable".into())
        },
        Condition::All { conditions } | Condition::Any { conditions } => conditions
            .iter()
            .find_map(|condition| unavailable(condition, bound)),
        Condition::Not { condition } => unavailable(condition, bound),
        _ => None,
    }
}
impl BoundContext {
    /// Unknown inputs suppress the entire rule, including under Not/Any.
    /// Validate the original set before filtering; never silently repair bad rules.
    pub fn resolve(&self, rules: &RuleSet) -> Result<Resolution, Vec<Issue>> {
        rules.validate()?;
        let mut adapted = rules.clone();
        let mut suppressed = BTreeMap::new();
        for rule in &mut adapted.rules {
            if rule.enabled
                && rule.target.kind == self.context.kind
                && (rule.target.ids.is_empty() || rule.target.ids.contains(&self.context.id))
            {
                if let Some(reason) = unavailable(&rule.condition, self) {
                    suppressed.insert(rule.id.clone(), reason);
                    rule.enabled = false;
                }
            }
        }
        let mut resolution = adapted.resolve(&self.context)?;
        for trace in &mut resolution.traces {
            if let Some(reason) = suppressed.get(&trace.rule_id) {
                trace.reason = format!("Host input unavailable: {reason}; rule skipped");
            }
        }
        Ok(resolution)
    }
    pub fn supports(&self, property: Property) -> bool {
        matches!(
            (&self.occurrence, property),
            (Occurrence::Forme { .. }, Property::RegionGuides)
        )
    }
    pub fn guides(&self, resolution: &Resolution) -> Option<(Uuid, bool)> {
        let Occurrence::Forme { id } = self.occurrence else {
            return None;
        };
        let decision = resolution.winners.get(&Property::RegionGuides)?;
        match decision.effect {
            Effect::RegionGuides { visible } => Some((id, visible)),
            _ => None,
        }
    }
}
#[cfg(test)]
#[path = "presentation_binding_tests.rs"]
mod tests;
