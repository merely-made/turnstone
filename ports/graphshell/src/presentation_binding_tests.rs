// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::presentation_editor::presentation::{Rule, Target};
use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
use mere::kernel::graph::{
    Field, ScalarField, SemanticStatementSpec, SemanticSubKind, predicate_iri,
};
use mere::kernel::types::GraphScope;
fn fixture() -> (Graph, [Uuid; 4]) {
    let mut graph = Graph::new();
    let ids = [1, 2, 3, 4].map(Uuid::from_u128);
    for (id, address) in ids.into_iter().zip([
        "https://source.test/",
        "https://target.test/",
        "https://source.test/",
        "https://target.test/",
    ]) {
        add_node(&mut graph, Some(id), address.into(), Default::default());
    }
    (graph, ids)
}
fn rules(kind: PrimitiveKind, condition: Condition, effect: Effect) -> RuleSet {
    RuleSet {
        version: 1,
        rules: vec![Rule {
            id: "test".into(),
            label: "test".into(),
            enabled: true,
            priority: 0,
            target: Target {
                kind,
                ids: BTreeSet::new(),
            },
            condition,
            effects: vec![effect],
        }],
    }
}
fn forme() -> FormeFacts {
    FormeFacts {
        id: Uuid::from_u128(44),
        label: Some("Workbench".into()),
        locked: true,
        visible: true,
        members: 2,
    }
}
#[test]
fn equal_addresses_share_resource_facts_but_not_occurrences_or_states() {
    let (graph, [a, _, c, _]) = fixture();
    let before = serde_json::to_value(graph.to_snapshot()).unwrap();
    let a = bind(
        &graph,
        &Occurrence::Access { member: a },
        &ViewInputs {
            selected: Some(true),
            position_pinned: Some(false),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let c = bind(
        &graph,
        &Occurrence::Access { member: c },
        &ViewInputs {
            selected: Some(false),
            position_pinned: Some(true),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert_ne!(a.context.id, c.context.id);
    assert_eq!(
        a.context.facts["resource_id"],
        c.context.facts["resource_id"]
    );
    assert!(a.context.states.contains(&ViewState::Selected));
    assert!(!c.context.states.contains(&ViewState::Selected));
    assert!(c.context.states.contains(&ViewState::PositionPinned));
    assert!(!a.context.facts.contains_key("media"));
    assert_eq!(serde_json::to_value(graph.to_snapshot()).unwrap(), before);
}
#[test]
fn one_resource_assertion_has_distinct_exact_displayed_pair_occurrences() {
    let (mut graph, [a, b, c, d]) = fixture();
    let statement = graph
        .try_assert_semantic_statement(
            graph.get_node_key_by_id(a).unwrap(),
            graph.get_node_key_by_id(b).unwrap(),
            SemanticStatementSpec {
                predicate: predicate_iri(SemanticSubKind::Cites).into(),
                recognized_sub_kind: Some(SemanticSubKind::Cites),
                label: None,
                basis: Default::default(),
                graph_scope: GraphScope::Custom("urn:graph:review".into()),
                provenance_iri: Some("urn:author:alice".into()),
                asserted_at_ms: Some(17),
            },
        )
        .unwrap()
        .1
        .statement_id;
    let ab = Occurrence::Assertion {
        pair: LinkTarget::new(a, b),
        statement: statement.clone(),
    };
    let cd = Occurrence::Assertion {
        pair: LinkTarget::new(c, d),
        statement,
    };
    let first = bind(&graph, &ab, &ViewInputs::default(), None).unwrap();
    let second = bind(&graph, &cd, &ViewInputs::default(), None).unwrap();
    assert_ne!(first.context.id, second.context.id);
    assert_eq!(
        first.context.facts["source_from"],
        second.context.facts["source_from"]
    );
    assert_eq!(
        first.context.facts["predicate"],
        second.context.facts["predicate"]
    );
    assert!(!first.context.facts.contains_key("derived"));
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplaySetShownResourceById {
            surface_id: c,
            resource_id: None,
        },
    );
    assert!(bind(&graph, &cd, &ViewInputs::default(), None).is_err());
    assert!(bind(&graph, &ab, &ViewInputs::default(), None).is_ok());
}
#[test]
fn assertion_binding_preserves_host_source_and_graph_only_binding_discloses_unavailable() {
    let (mut graph, [a, b, _, _]) = fixture();
    let statement = graph
        .try_assert_semantic_statement(
            graph.get_node_key_by_id(a).unwrap(),
            graph.get_node_key_by_id(b).unwrap(),
            SemanticStatementSpec {
                predicate: predicate_iri(SemanticSubKind::Cites).into(),
                recognized_sub_kind: Some(SemanticSubKind::Cites),
                label: None,
                basis: Default::default(),
                graph_scope: GraphScope::Default,
                provenance_iri: None,
                asserted_at_ms: None,
            },
        )
        .unwrap()
        .1
        .statement_id;
    let occurrence = Occurrence::Assertion {
        pair: LinkTarget::new(a, b),
        statement,
    };
    let source = scenograph::link::LinkSource::Available {
        binding: scenograph::SourceBinding {
            authority: "mere".into(),
            domain: "graph-session".into(),
            resource: "session:123/graph:456".into(),
        },
        revision: scenograph::link::LinkSourceRevision::Unavailable {
            reason: "live source has no qualified immutable revision".into(),
        },
    };

    let bound = bind_with_source(
        &graph,
        &occurrence,
        &ViewInputs::default(),
        None,
        source.clone(),
    )
    .unwrap();
    assert_eq!(bound.link_source, Some(source));

    let graph_only = bind(&graph, &occurrence, &ViewInputs::default(), None).unwrap();
    assert!(matches!(
        graph_only.link_source,
        Some(scenograph::link::LinkSource::Unavailable { .. })
    ));
}

#[test]
fn removed_access_cannot_retarget_equal_address() {
    let (mut graph, [a, _, c, _]) = fixture();
    let occurrence = Occurrence::Access { member: a };
    let key = graph.get_node_key_by_id(a).unwrap();
    apply_graph_delta(&mut graph, GraphDelta::RemoveNode { key });
    assert!(bind(&graph, &occurrence, &ViewInputs::default(), None).is_err());
    assert!(
        bind(
            &graph,
            &Occurrence::Access { member: c },
            &ViewInputs::default(),
            None
        )
        .is_ok()
    );
}
#[test]
fn unknown_negated_state_and_measurement_never_match() {
    let (graph, [a, _, _, _]) = fixture();
    let bound = bind(
        &graph,
        &Occurrence::Access { member: a },
        &ViewInputs {
            selected: Some(false),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    for condition in [
        Condition::Not {
            condition: Box::new(Condition::State {
                state: ViewState::ForegroundPinned,
            }),
        },
        Condition::Not {
            condition: Box::new(Condition::ScreenSize {
                enter_below: 24.,
                exit_above: 48.,
            }),
        },
        Condition::Any {
            conditions: vec![
                Condition::Always,
                Condition::State {
                    state: ViewState::Overview,
                },
            ],
        },
    ] {
        let result = bound
            .resolve(&rules(
                PrimitiveKind::Node,
                condition,
                Effect::Details { visible: true },
            ))
            .unwrap();
        assert!(result.winners.is_empty());
        assert!(result.traces[0].reason.contains("unavailable"));
    }
    let known_false = Condition::Not {
        condition: Box::new(Condition::State {
            state: ViewState::Selected,
        }),
    };
    assert!(
        !bound
            .resolve(&rules(
                PrimitiveKind::Node,
                known_false,
                Effect::Details { visible: true }
            ))
            .unwrap()
            .winners
            .is_empty()
    );
}
#[test]
fn form_owner_keeps_numen_identity_and_supported_effects_separate() {
    let (mut graph, _) = fixture();
    let forme = forme();
    let id = FieldId::from_uuid(forme.id);
    apply_graph_delta(
        &mut graph,
        GraphDelta::AddField {
            field: Field::new(id, FieldDefinition::Scalar(ScalarField::Const(0.))),
        },
    );
    let field = bind(
        &graph,
        &Occurrence::Numen { id },
        &ViewInputs::default(),
        None,
    )
    .unwrap();
    let bound = bind(
        &graph,
        &Occurrence::Forme { id: forme.id },
        &ViewInputs::default(),
        Some(&forme),
    )
    .unwrap();
    assert_ne!(field.context.id, bound.context.id);
    let rules = rules(
        PrimitiveKind::Field,
        Condition::Always,
        Effect::RegionGuides { visible: false },
    );
    assert_eq!(field.guides(&field.resolve(&rules).unwrap()), None);
    assert_eq!(
        bound.guides(&bound.resolve(&rules).unwrap()),
        Some((forme.id, false))
    );
    assert!(!bound.supports(Property::DropPreviews));
    assert!(!field.supports(Property::RegionGuides));
    assert!(
        bind(
            &graph,
            &Occurrence::Forme {
                id: Uuid::from_u128(99)
            },
            &ViewInputs::default(),
            Some(&forme)
        )
        .is_err()
    );
}
#[test]
fn live_forme_unlock_and_hover_inputs_use_current_view() {
    let (graph, _) = fixture();
    let mut forme = forme();
    let rules = rules(
        PrimitiveKind::Field,
        Condition::State {
            state: ViewState::LayoutUnlocked,
        },
        Effect::RegionGuides { visible: true },
    );
    let occurrence = Occurrence::Forme { id: forme.id };
    let locked = bind(
        &graph,
        &occurrence,
        &ViewInputs {
            hovered: Some(false),
            ..Default::default()
        },
        Some(&forme),
    )
    .unwrap();
    assert!(locked.resolve(&rules).unwrap().winners.is_empty());
    forme.locked = false;
    let unlocked = bind(
        &graph,
        &occurrence,
        &ViewInputs {
            hovered: Some(true),
            ..Default::default()
        },
        Some(&forme),
    )
    .unwrap();
    assert_eq!(
        unlocked.guides(&unlocked.resolve(&rules).unwrap()),
        Some((forme.id, true))
    );
    assert_eq!(unlocked.context.facts["hovered"], Scalar::Bool(true));
    assert!(!unlocked.available_states.contains(&ViewState::Selected));
}
#[test]
fn invalid_rule_is_not_hidden_by_unavailable_input_filter() {
    let (graph, [a, _, _, _]) = fixture();
    let bound = bind(
        &graph,
        &Occurrence::Access { member: a },
        &ViewInputs::default(),
        None,
    )
    .unwrap();
    let mut rules = rules(
        PrimitiveKind::Node,
        Condition::State {
            state: ViewState::Overview,
        },
        Effect::Details { visible: true },
    );
    rules.rules[0]
        .effects
        .push(Effect::RegionGuides { visible: true });
    assert!(bound.resolve(&rules).is_err());
}
#[test]
fn attention_binds_as_states_only_when_supplied() {
    use scenograph::attention::Attention;
    let (graph, [a, ..]) = fixture();
    let access = Occurrence::Access { member: a };
    let mut view = ViewInputs::default();
    view.set_attention(Some(Attention::Retained), Some(false));
    let retained = bind(&graph, &access, &view, None).unwrap();
    assert!(retained.context.states.contains(&ViewState::Retained));
    assert!(!retained.context.states.contains(&ViewState::Suggested));
    for state in [
        ViewState::ForegroundPinned,
        ViewState::Retained,
        ViewState::Suggested,
    ] {
        assert!(retained.available_states.contains(&state));
    }
    view.set_attention(Some(Attention::Forward), Some(true));
    let pinned = bind(&graph, &access, &view, None).unwrap();
    assert!(pinned.context.states.contains(&ViewState::ForegroundPinned));
    assert!(!pinned.context.states.contains(&ViewState::Retained));
    let unknown = bind(&graph, &access, &ViewInputs::default(), None).unwrap();
    assert!(!unknown.available_states.contains(&ViewState::Retained));
    assert!(
        unknown
            .notes
            .iter()
            .any(|note| note.contains("Overview state has no adapter"))
    );
}
