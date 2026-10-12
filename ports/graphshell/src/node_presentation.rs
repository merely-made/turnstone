// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Bounded node face decisions and view/occurrence-local hysteresis.
use crate::presentation_binding::{BoundContext, Occurrence};
use crate::presentation_editor::{
    RuleSetDraft,
    presentation::{Effect, PrimitiveKind, Property, Resolution, RuleSet, ViewState},
};
pub use sceno::Representation;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
pub const MAX_LIVE_NODES: usize = 128;
const MAX_CONTEXT_BYTES: usize = 256 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NodeDecision {
    pub member: Uuid,
    pub bound: BoundContext,
    pub resolution: Resolution,
    pub face: Option<Effect>,
    pub attributes: bool,
    pub unsupported: Vec<String>,
}
#[derive(Default)]
pub struct NodeRuntime {
    scope: Option<(u64, u64)>,
    inputs: Vec<BoundContext>,
    rules: Option<RuleSet>,
    previous: BTreeMap<String, BTreeSet<String>>,
    decisions: Vec<NodeDecision>,
}
impl NodeRuntime {
    pub fn same_inputs(
        &self,
        epoch: u64,
        generation: u64,
        inputs: &[BoundContext],
        rules: &RuleSet,
    ) -> bool {
        self.scope == Some((epoch, generation))
            && self.inputs == inputs
            && self.rules.as_ref() == Some(rules)
    }
    pub fn decisions(&self) -> &[NodeDecision] {
        &self.decisions
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn evaluate(
        &mut self,
        view_epoch: u64,
        generation: u64,
        inputs: &[BoundContext],
        rules: &RuleSet,
    ) -> Result<bool, String> {
        let result = self.prepare(view_epoch, generation, inputs, rules);
        if result.is_err() {
            self.clear();
        }
        result
    }
    fn prepare(
        &mut self,
        epoch: u64,
        generation: u64,
        inputs: &[BoundContext],
        rules: &RuleSet,
    ) -> Result<bool, String> {
        rules
            .validate()
            .map_err(|issues| format!("Saved node rules refused: {issues:?}"))?;
        if inputs.len() > MAX_LIVE_NODES {
            return Err(
                "Live node rules need at most 128 visible accesses; base faces restored".into(),
            );
        }
        if serde_json::to_vec(inputs)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_CONTEXT_BYTES
        {
            return Err("Live node contexts exceed 256 KiB; base faces restored".into());
        }
        let scope = (epoch, generation);
        if self.scope == Some(scope) && self.inputs == inputs && self.rules.as_ref() == Some(rules)
        {
            return Ok(false);
        }
        let reuse = self.scope == Some(scope) && self.rules.as_ref() == Some(rules);
        let mut decisions = Vec::new();
        let mut next = BTreeMap::new();
        for input in inputs {
            let Occurrence::Access { member } = input.occurrence else {
                return Err("Node runtime requires exact access occurrences".into());
            };
            if input.context.kind != PrimitiveKind::Node
                || input.context.id != input.occurrence.id()
                || next.contains_key(&input.context.id)
            {
                return Err("Node occurrence identity is inconsistent or duplicated".into());
            }
            let mut bound = input.clone();
            // The caller cannot seed hidden cross-view matches.
            bound.context.previous_matches = if reuse {
                self.previous
                    .get(&bound.context.id)
                    .cloned()
                    .unwrap_or_default()
            } else {
                BTreeSet::new()
            };
            let resolution = bound
                .resolve(rules)
                .map_err(|issues| format!("Node rules refused: {issues:?}"))?;
            let face = resolution
                .winners
                .get(&Property::Representation)
                .filter(|winner| bound.supports_effect(&winner.effect))
                .map(|winner| winner.effect.clone());
            let attributes = bound.context.states.contains(&ViewState::Selected)
                && resolution
                    .winners
                    .get(&Property::Details)
                    .is_some_and(|winner| {
                        matches!(winner.effect, Effect::Details { visible: true })
                    });
            let unsupported = resolution
                .winners
                .values()
                .filter(|winner| !bound.supports_effect(&winner.effect))
                .map(|winner| {
                    format!(
                        "{} from {} has no node renderer here; base face remains",
                        serde_json::to_string(&winner.effect).unwrap(),
                        winner.rule_id
                    )
                })
                .collect();
            next.insert(bound.context.id.clone(), resolution.matched_rules.clone());
            decisions.push(NodeDecision {
                member,
                bound,
                resolution,
                face,
                attributes,
                unsupported,
            });
        }
        self.previous = next;
        self.decisions = decisions;
        self.scope = Some(scope);
        self.inputs = inputs.to_vec();
        self.rules = Some(rules.clone());
        Ok(true)
    }
}
/// Ordinary Canvas face, compact icon, selected attributes, positional-pin guard.
/// Users can edit all priorities, thresholds and conditions before applying.
pub fn add_defaults(draft: &mut RuleSetDraft) -> usize {
    let definitions = [
        (
            "Base node face",
            0,
            json!({"kind":"always"}),
            json!([{"kind":"representation","value":"Card"}]),
        ),
        (
            "Compact node face",
            10,
            json!({"kind":"all","conditions":[{"kind":"screen_size","enter_below":40,"exit_above":56},{"kind":"not","condition":{"kind":"state","state":"position_pinned"}}]}),
            json!([{"kind":"representation","value":"Glyph"}]),
        ),
        (
            "Selected node attributes",
            20,
            json!({"kind":"state","state":"selected"}),
            json!([{"kind":"representation","value":"Card"},{"kind":"details","visible":true}]),
        ),
        (
            "Position-pinned node face",
            30,
            json!({"kind":"state","state":"position_pinned"}),
            json!([{"kind":"representation","value":"Card"}]),
        ),
    ];
    let mut selected = 0;
    for (label, priority, condition, effects) in definitions {
        selected = draft.add(PrimitiveKind::Node);
        let row = &mut draft.rules[selected];
        row.label = label.into();
        row.priority = priority.to_string();
        row.condition = condition.to_string();
        row.effects = effects.to_string();
    }
    selected
}
#[cfg(test)]
#[path = "node_presentation_tests.rs"]
mod tests;
