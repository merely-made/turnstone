// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn typed_journal_authors_preserve_no_self_wake_subject_matching() {
    use mere::kernel::graph::Author;
    let app = App::test_stub();
    let subject = Subject::new([0x31; 32]);
    let node = uuid::Uuid::new_v4().to_string();
    let body_author = Author::script(subject.to_hex(), "body-1").via("turnstone");
    {
        let mut journal = app.journal.lock().unwrap();
        for author in [Author::user(), body_author.clone()] {
            journal
                .record_as(
                    app.behavior_binding.unwrap(),
                    author,
                    CapturedDelta::ReplaySetNodeTitleById {
                        node_id: node.clone(),
                        title: "changed".into(),
                    },
                )
                .unwrap();
        }
    }
    let entries = entries_since(&app, 0);
    let watch = servitor::Watch {
        subject,
        scope: ScopePath::parse(&node).unwrap(),
        self_author: subject.to_hex(),
        cursor: 0,
    };
    assert!(
        watch.matches(&entries[0].as_event()),
        "the human edit wakes the body"
    );
    assert!(
        !watch.matches(&entries[1].as_event()),
        "the body's own edit never wakes itself"
    );
    assert_eq!(entries[1].author, subject.to_hex());
    assert_eq!(
        app.journal.lock().unwrap().entries()[1].author,
        body_author,
        "the string matcher projection leaves full attribution in the journal"
    );
}

#[test]
fn a_delta_naming_one_node_yields_that_node() {
    let delta = CapturedDelta::ReplaySetNodeTitleById {
        node_id: "n1".into(),
        title: "t".into(),
    };
    assert_eq!(touched_ids(&delta), vec!["n1"]);
}

#[test]
fn a_relation_delta_yields_both_ends() {
    let delta = CapturedDelta::ReplayBranchHistoryByIds {
        child_id: "child".into(),
        parent_id: "parent".into(),
    };
    assert_eq!(touched_ids(&delta), vec!["child", "parent"]);
    let undo = CapturedDelta::ReplaySetEdgesByIds {
        from_id: "child".into(),
        to_id: "parent".into(),
        edges: Vec::new(),
    };
    assert_eq!(touched_ids(&undo), vec!["child", "parent"]);
}

#[test]
fn a_field_delta_names_no_node() {
    // The physics tier is outside the behavior vocabulary on purpose.
    let delta = CapturedDelta::ReplaySetImportRecords {
        import_records: Vec::new(),
    };
    assert!(touched_ids(&delta).is_empty());
    assert!(
        touched_ids(&CapturedDelta::ReplayRemoveFieldById {
            field_id: "field".into(),
        })
        .is_empty()
    );
}

#[test]
fn a_node_the_graph_does_not_hold_falls_back_to_its_bare_id() {
    let graph = Graph::new();
    let id = uuid::Uuid::new_v4().to_string();
    let scopes = ancestry_scopes(&graph, &id);
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0].segments(), &[id]);
}

#[test]
fn a_digest_names_the_nodes_that_changed_not_their_containers() {
    // Ancestry is written outermost-first, so the tail of each scope is
    // the node that actually moved. A body wants that, not the folder.
    let entries = vec![
        CommittedEntry::new(4, "user", vec![ScopePath::parse("folder/leaf").unwrap()]),
        CommittedEntry::new(5, "user", vec![ScopePath::parse("elsewhere").unwrap()]),
    ];
    let wake = servitor::Wake {
        subject: Subject::new([1; 32]),
        matched: vec![4],
    };
    let context = context_for(&entries, &wake);
    assert_eq!(context.woken_by.len(), 1, "only the matched entry");
    assert_eq!(context.woken_by[0].seq, 4);
    assert_eq!(context.woken_by[0].nodes, vec!["leaf".to_string()]);
    assert_eq!(context.woken_by[0].author, "user");
}

#[test]
fn an_unwoken_context_is_empty_rather_than_absent() {
    let context = TriggerContext::default();
    assert!(context.is_empty());
    assert_eq!(context.to_json(), r#"{"woken_by":[]}"#);
}

#[test]
fn a_malformed_id_yields_no_scope_rather_than_a_bogus_one() {
    let graph = Graph::new();
    assert!(ancestry_scopes(&graph, "not-a-uuid").is_empty());
}

fn resource_views() -> (
    Graph,
    mere::kernel::graph::NodeKey,
    mere::kernel::graph::NodeKey,
) {
    use mere::kernel::graph::apply::add_node;
    let mut graph = Graph::new();
    let a = add_node(
        &mut graph,
        None,
        "https://example.test/page#one".into(),
        Default::default(),
    );
    let b = add_node(
        &mut graph,
        None,
        "https://example.test/page#two".into(),
        Default::default(),
    );
    assert_eq!(graph.shown_resource_id(a), graph.shown_resource_id(b));
    (graph, a, b)
}

fn surface_names(graph: &Graph, keys: &[mere::kernel::graph::NodeKey]) -> Vec<String> {
    let mut ids: Vec<_> = keys
        .iter()
        .map(|key| graph.get_node(*key).unwrap().id.to_string())
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn resource_record_wakes_each_current_view_without_resource_uuid_scopes() {
    let (graph, a, b) = resource_views();
    let resource = graph.shown_resource_id(a).unwrap();
    let delta = CapturedDelta::ReplaySetResourceRecordById {
        resource_id: resource.to_string(),
        record: None,
    };
    assert!(touched_ids(&delta).is_empty());
    assert_eq!(
        touched_surface_ids(&graph, &delta),
        surface_names(&graph, &[a, b])
    );
    assert!(!touched_surface_ids(&graph, &delta).contains(&resource.to_string()));
    assert!(
        touched_surface_ids(&Graph::new(), &delta).is_empty(),
        "a detached Resource never becomes a bare Surface watch scope"
    );
}

#[test]
fn concept_record_changes_wake_incoming_tag_sources_even_after_facet_removal() {
    use mere::kernel::graph::apply::{
        GraphDelta, add_node, apply_graph_delta, assert_semantic_predicate_in_scope,
    };
    use mere::kernel::graph::resource::ResourceNode;
    use mere::kernel::graph::resource_tags::TagConcept;
    use mere::kernel::persistence::PersistedResourceRecord;
    let (mut graph, a, b) = resource_views();
    let resource = graph.shown_resource_id(a).unwrap();
    let concept_iri = "https://vocabulary.test/tags#Research";
    assert!(
        graph
            .tag_resource_with_concept(
                resource,
                concept_iri,
                TagConcept {
                    owner_iri: "urn:author:alice".into(),
                    label: "Research".into(),
                }
            )
            .unwrap()
    );
    let concept = ResourceNode::for_term(concept_iri).id();
    let concept_view = add_node(
        &mut graph,
        None,
        "https://example.test/concept".into(),
        Default::default(),
    );
    let concept_surface = graph.get_node(concept_view).unwrap().id;
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplaySetShownResourceById {
            surface_id: concept_surface,
            resource_id: Some(concept),
        },
    );
    let unrelated = add_node(
        &mut graph,
        None,
        "https://unrelated.test/page".into(),
        Default::default(),
    );
    assert!(
        assert_semantic_predicate_in_scope(
            &mut graph,
            unrelated,
            concept_view,
            "https://unrelated.test/taggedWith".into(),
            Default::default()
        )
        .is_some()
    );
    let record = PersistedResourceRecord {
        canonical_iri: concept_iri.into(),
        facets: Vec::new(),
    };
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplaySetResourceRecordById {
            resource_id: concept,
            record: Some(record.clone()),
        },
    );
    assert!(graph.resource_tag_concept(concept).is_none());
    assert!(graph.node_content_tags(a).unwrap().is_empty());
    let delta = CapturedDelta::ReplaySetResourceRecordById {
        resource_id: concept.to_string(),
        record: Some(record),
    };
    assert_eq!(
        touched_surface_ids(&graph, &delta),
        surface_names(&graph, &[a, b, concept_view]),
        "incoming exact taggedWith sources and the concept view wake; arbitrary incoming edges do not"
    );
}

#[test]
fn retracted_resource_pair_still_wakes_both_endpoint_views_once() {
    use mere::kernel::graph::apply::{GraphDelta, add_node, apply_graph_delta};
    use mere::kernel::graph::resource::ResourceNode;
    use mere::kernel::graph::resource_tags::TagConcept;
    let (mut graph, a, b) = resource_views();
    let resource = graph.shown_resource_id(a).unwrap();
    let concept_iri = "https://vocabulary.test/tags#Research";
    graph
        .tag_resource_with_concept(
            resource,
            concept_iri,
            TagConcept {
                owner_iri: "urn:author:alice".into(),
                label: "Research".into(),
            },
        )
        .unwrap();
    let concept = ResourceNode::for_term(concept_iri).id();
    let c = add_node(
        &mut graph,
        None,
        "https://example.test/concept-view".into(),
        Default::default(),
    );
    let c_id = graph.get_node(c).unwrap().id;
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplaySetShownResourceById {
            surface_id: c_id,
            resource_id: Some(concept),
        },
    );
    let delta = CapturedDelta::ReplaySetResourceEdgesByIds {
        from_resource_id: resource.to_string(),
        to_resource_id: concept.to_string(),
        edges: Vec::new(),
    };
    let mere::kernel::graph::capture::Replay::Apply(replay) = delta.replay() else {
        panic!("resource pair replay record must produce a graph delta");
    };
    apply_graph_delta(&mut graph, replay);
    assert!(
        graph
            .resource_edges()
            .all(|(from, to, _)| from.id() != resource || to.id() != concept)
    );
    assert_eq!(
        touched_surface_ids(&graph, &delta),
        surface_names(&graph, &[a, b, c])
    );
    let self_pair = CapturedDelta::ReplaySetResourceEdgesByIds {
        from_resource_id: resource.to_string(),
        to_resource_id: resource.to_string(),
        edges: Vec::new(),
    };
    assert_eq!(
        touched_surface_ids(&graph, &self_pair),
        surface_names(&graph, &[a, b])
    );
}

#[test]
fn shown_resource_changes_keep_the_surface_even_after_detach_or_removal() {
    use mere::kernel::graph::apply::{GraphDelta, apply_graph_delta};
    let (mut graph, a, b) = resource_views();
    let resource = graph.shown_resource_id(a).unwrap();
    let a_id = graph.get_node(a).unwrap().id;
    let record = CapturedDelta::ReplaySetResourceRecordById {
        resource_id: resource.to_string(),
        record: None,
    };
    let detach = CapturedDelta::ReplaySetShownResourceById {
        surface_id: a_id.to_string(),
        resource_id: None,
    };
    let mere::kernel::graph::capture::Replay::Apply(replay) = detach.replay() else {
        panic!("detach replay record must produce a graph delta");
    };
    apply_graph_delta(&mut graph, replay);
    assert_eq!(
        touched_surface_ids(&graph, &record),
        surface_names(&graph, &[b]),
        "Resource records project through final state, not old mappings"
    );
    assert_eq!(touched_surface_ids(&graph, &detach), vec![a_id.to_string()]);
    apply_graph_delta(
        &mut graph,
        GraphDelta::ReplayRemoveNodeById { node_id: a_id },
    );
    let ids = touched_surface_ids(&graph, &detach);
    assert_eq!(ids, vec![a_id.to_string()]);
    assert_eq!(
        ancestry_scopes(&graph, &ids[0])[0].segments(),
        &[a_id.to_string()]
    );
}

#[test]
fn resource_projection_preserves_journal_author_and_no_self_wake() {
    use mere::kernel::graph::Author;
    let (graph, a, b) = resource_views();
    let resource = graph.shown_resource_id(a).unwrap();
    let expected = surface_names(&graph, &[a, b]);
    let mut app = App::test_stub();
    let graph_id = app.graph_runtimes.active_graph();
    app.graph_runtimes.activate_or_insert(
        graph_id,
        Some(app.session_id),
        mere::canvas::Canvas::with_graph(graph),
    );
    app.bind_behavior_journal().unwrap();
    let cursor = journal_high_water(&app);
    let subject = Subject::new([0x42; 32]);
    let author = Author::script(subject.to_hex(), "resource-body").via("turnstone");
    {
        let mut journal = app.journal.lock().unwrap();
        journal
            .record_as(
                app.behavior_binding.unwrap(),
                author.clone(),
                CapturedDelta::ReplaySetResourceRecordById {
                    resource_id: resource.to_string(),
                    record: None,
                },
            )
            .unwrap();
    }
    let entries = entries_since(&app, cursor);
    assert_eq!(entries.len(), 1);
    let wake = servitor::Wake {
        subject,
        matched: vec![cursor + 1],
    };
    assert_eq!(context_for(&entries, &wake).woken_by[0].nodes, expected);
    for surface in &expected {
        let watch = servitor::Watch {
            subject,
            scope: ScopePath::parse(surface).unwrap(),
            self_author: subject.to_hex(),
            cursor,
        };
        assert!(!watch.matches(&entries[0].as_event()));
    }
    assert_eq!(entries[0].author, subject.to_hex());
    assert_eq!(
        app.journal.lock().unwrap().entries().last().unwrap().author,
        author
    );
}

#[test]
fn resource_and_surface_containment_both_contribute_current_watch_ancestry() {
    use mere::kernel::graph::apply::{add_node, assert_relation};
    use mere::kernel::graph::{ContainmentSubKind, EdgeAssertion};
    let (mut graph, a, _) = resource_views();
    let parent = add_node(
        &mut graph,
        None,
        "https://containers.test/collection#one".into(),
        Default::default(),
    );
    let parent_alias = add_node(
        &mut graph,
        None,
        "https://containers.test/collection#two".into(),
        Default::default(),
    );
    let folder = add_node(
        &mut graph,
        None,
        "https://folders.test/surface".into(),
        Default::default(),
    );
    assert_relation(
        &mut graph,
        a,
        parent,
        EdgeAssertion::Containment {
            sub_kind: ContainmentSubKind::UrlPath,
        },
    )
    .unwrap();
    assert_relation(
        &mut graph,
        a,
        folder,
        EdgeAssertion::Containment {
            sub_kind: ContainmentSubKind::UserFolder,
        },
    )
    .unwrap();
    let leaf = graph.get_node(a).unwrap().id.to_string();
    let scopes = ancestry_scopes(&graph, &leaf);
    for key in [parent, parent_alias, folder] {
        let parent_id = graph.get_node(key).unwrap().id.to_string();
        assert!(
            scopes
                .iter()
                .any(|scope| scope.segments() == &[parent_id.clone(), leaf.clone()]),
            "both views of the resource container and the Surface folder are distinct regions"
        );
    }
}
