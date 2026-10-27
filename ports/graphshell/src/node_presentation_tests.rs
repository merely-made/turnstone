// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::presentation_binding::{ViewInputs, bind};
use mere::kernel::graph::Graph;
use mere::kernel::graph::apply::add_node;
use scenograph::presentation::Scalar;
fn fixture() -> (Graph, Uuid, Uuid, RuleSet) {
    let mut graph = Graph::new();
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    for id in [a, b] {
        add_node(
            &mut graph,
            Some(id),
            "https://same.test/".into(),
            Default::default(),
        );
    }
    let mut draft = RuleSetDraft::default();
    add_defaults(&mut draft);
    (graph, a, b, draft.validate().unwrap())
}
fn input(graph: &Graph, member: Uuid, size: f64, selected: bool, pinned: bool) -> BoundContext {
    bind(
        graph,
        &Occurrence::Access { member },
        &ViewInputs {
            screen_size: Some(size),
            selected: Some(selected),
            position_pinned: Some(pinned),
            ..Default::default()
        },
        None,
    )
    .unwrap()
}
fn face(decision: &NodeDecision) -> String {
    decision
        .face
        .as_ref()
        .map(|effect| serde_json::to_string(effect).unwrap())
        .unwrap_or_default()
}
#[test]
fn repeated_resources_keep_independent_occurrence_hysteresis() {
    let (graph, a, b, rules) = fixture();
    let mut runtime = NodeRuntime::default();
    runtime
        .evaluate(
            1,
            1,
            &[
                input(&graph, a, 30., false, false),
                input(&graph, b, 46., false, false),
            ],
            &rules,
        )
        .unwrap();
    assert!(face(&runtime.decisions()[0]).contains("Glyph"));
    assert!(face(&runtime.decisions()[1]).contains("Card"));
    runtime
        .evaluate(
            1,
            1,
            &[
                input(&graph, a, 46., false, false),
                input(&graph, b, 46., false, false),
            ],
            &rules,
        )
        .unwrap();
    assert!(face(&runtime.decisions()[0]).contains("Glyph"));
    assert!(face(&runtime.decisions()[1]).contains("Card"));
    assert_eq!(
        runtime.decisions()[0].bound.context.facts["resource_id"],
        runtime.decisions()[1].bound.context.facts["resource_id"]
    );
}
#[test]
fn selected_attributes_and_position_pin_guard_have_separate_meanings() {
    let (graph, a, b, rules) = fixture();
    let mut runtime = NodeRuntime::default();
    runtime
        .evaluate(
            1,
            1,
            &[
                input(&graph, a, 30., true, false),
                input(&graph, b, 30., false, true),
            ],
            &rules,
        )
        .unwrap();
    assert!(runtime.decisions()[0].attributes);
    assert!(!runtime.decisions()[1].attributes);
    assert!(
        runtime
            .decisions()
            .iter()
            .all(|decision| face(decision).contains("Card"))
    );
}
#[test]
fn graph_replacement_and_saved_generation_reset_hysteresis() {
    let (graph, a, _, rules) = fixture();
    let mut runtime = NodeRuntime::default();
    runtime
        .evaluate(1, 1, &[input(&graph, a, 30., false, false)], &rules)
        .unwrap();
    runtime
        .evaluate(2, 1, &[input(&graph, a, 46., false, false)], &rules)
        .unwrap();
    assert!(face(&runtime.decisions()[0]).contains("Card"));
    runtime
        .evaluate(2, 1, &[input(&graph, a, 30., false, false)], &rules)
        .unwrap();
    runtime
        .evaluate(2, 2, &[input(&graph, a, 46., false, false)], &rules)
        .unwrap();
    assert!(face(&runtime.decisions()[0]).contains("Card"));
}
#[test]
fn departed_occurrences_and_caller_matches_do_not_leak_into_reentry() {
    let (graph, a, b, rules) = fixture();
    let mut runtime = NodeRuntime::default();
    runtime
        .evaluate(1, 1, &[input(&graph, a, 30., false, false)], &rules)
        .unwrap();
    runtime
        .evaluate(1, 1, &[input(&graph, b, 30., false, false)], &rules)
        .unwrap();
    let mut returning = input(&graph, a, 46., false, false);
    returning.context.previous_matches.insert("node-2".into());
    runtime.evaluate(1, 1, &[returning], &rules).unwrap();
    assert!(face(&runtime.decisions()[0]).contains("Card"));
}
#[test]
fn unsupported_snapshot_winner_preserves_base_instead_of_lower_rule() {
    let (graph, a, _, mut rules) = fixture();
    let mut row = rules.rules[0].clone();
    row.id = "snapshot".into();
    row.priority = 100;
    row.effects =
        serde_json::from_value(json!([{"kind":"representation","value":"Snapshot"}])).unwrap();
    rules.rules.push(row);
    let mut runtime = NodeRuntime::default();
    runtime
        .evaluate(1, 1, &[input(&graph, a, 30., false, false)], &rules)
        .unwrap();
    assert!(runtime.decisions()[0].face.is_none());
    assert_eq!(runtime.decisions()[0].unsupported.len(), 1);
    assert_eq!(
        runtime.decisions()[0].resolution.winners[&Property::Representation].rule_id,
        "snapshot"
    );
}
#[test]
fn unavailable_negated_state_cannot_trigger_live_attributes() {
    let (graph, a, _, mut rules) = fixture();
    rules.rules[2].condition = serde_json::from_value(
        json!({"kind":"not","condition":{"kind":"state","state":"foreground_pinned"}}),
    )
    .unwrap();
    let mut runtime = NodeRuntime::default();
    runtime
        .evaluate(1, 1, &[input(&graph, a, 30., true, false)], &rules)
        .unwrap();
    assert!(!runtime.decisions()[0].attributes);
    assert!(
        runtime.decisions()[0].resolution.traces[2]
            .reason
            .contains("unavailable")
    );
}
#[test]
fn source_inputs_are_read_only_and_unchanged_inputs_do_not_rebuild() {
    let (graph, a, _, rules) = fixture();
    let inputs = vec![input(&graph, a, 30., true, false)];
    let before = inputs.clone();
    let source = serde_json::to_value(graph.to_snapshot()).unwrap();
    let mut runtime = NodeRuntime::default();
    assert!(runtime.evaluate(1, 1, &inputs, &rules).unwrap());
    assert!(!runtime.evaluate(1, 1, &inputs, &rules).unwrap());
    assert_eq!(inputs, before);
    assert_eq!(serde_json::to_value(graph.to_snapshot()).unwrap(), source);
}
#[test]
fn bounds_duplicate_ids_and_invalid_rules_refuse_atomically() {
    let (graph, a, _, rules) = fixture();
    let one = input(&graph, a, 30., false, false);
    let mut runtime = NodeRuntime::default();
    runtime.evaluate(1, 1, &[one.clone()], &rules).unwrap();
    assert!(
        runtime
            .evaluate(1, 1, &vec![one.clone(); MAX_LIVE_NODES + 1], &rules)
            .is_err()
    );
    assert!(runtime.decisions().is_empty());
    assert!(
        runtime
            .evaluate(1, 1, &[one.clone(), one.clone()], &rules)
            .is_err()
    );
    let mut huge = one.clone();
    huge.context
        .facts
        .insert("blob".into(), Scalar::Text("x".repeat(MAX_CONTEXT_BYTES)));
    assert!(runtime.evaluate(1, 1, &[huge], &rules).is_err());
    let mut invalid = rules.clone();
    invalid.rules[0].effects.clear();
    assert!(runtime.evaluate(1, 1, &[one], &invalid).is_err());
}
