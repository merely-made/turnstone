// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Full-host controls for graph capture and session-bound behavior delivery.
use super::*;
use crate::host_journal::{RuntimeOrigin, restore_table};
use mere::kernel::graph::apply::{add_node, assert_relation};
use mere::kernel::graph::{Author, Graph, capture::CapturedDelta};
use mere::kernel::graph::{ContainmentSubKind, EdgeAssertion};
use servitor::{ScopePath, Subject, Watch};

fn paired_graph(extra_view: bool, parent: u128) -> (Graph, uuid::Uuid) {
    let mut graph = Graph::new();
    let node = add_node(
        &mut graph,
        Some(uuid::Uuid::from_u128(7)),
        "https://shared.test/page#one".into(),
        Default::default(),
    );
    if extra_view {
        add_node(
            &mut graph,
            Some(uuid::Uuid::from_u128(8)),
            "https://shared.test/page#two".into(),
            Default::default(),
        );
    }
    let folder = add_node(
        &mut graph,
        Some(uuid::Uuid::from_u128(parent)),
        "mere://user-folder".into(),
        Default::default(),
    );
    assert_relation(
        &mut graph,
        node,
        folder,
        EdgeAssertion::Containment {
            sub_kind: ContainmentSubKind::UserFolder,
        },
    )
    .unwrap();
    let resource = graph.shown_resource_id(node).unwrap();
    (graph, resource)
}

fn record_resource(app: &App, origin: RuntimeOrigin, resource: uuid::Uuid) {
    app.journal
        .lock()
        .unwrap()
        .record_as(
            origin,
            Author::user(),
            CapturedDelta::ReplaySetResourceRecordById {
                resource_id: resource.to_string(),
                record: None,
            },
        )
        .unwrap();
}

#[test]
fn equal_resource_and_surface_ids_cannot_transfer_scopes_across_runtimes() {
    let mut app = App::test_stub();
    let (a, resource_a) = paired_graph(false, 10);
    let (b, resource_b) = paired_graph(true, 11);
    assert_eq!(resource_a, resource_b);
    let graph_a = app.graph_runtimes.active_graph();
    app.graph_runtimes.set_graph(a);
    app.bind_behavior_journal().unwrap();
    let origin_a = app.behavior_origin().unwrap();
    let cursor = app.journal.lock().unwrap().high_water();
    let graph_b = GraphId::from_uuid(uuid::Uuid::from_u128(2));
    app.graph_runtimes
        .activate_or_insert(graph_b, None, mere::canvas::Canvas::with_graph(b));
    let origin_b = app.graph_runtimes.get(graph_b).unwrap().origin().unwrap();
    record_resource(&app, origin_b, resource_b);
    let foreign = crate::behaviors::entries_since(&app, cursor);
    assert!(
        foreign.is_empty(),
        "a foreign resource edit cannot wake even a root watch"
    );
    record_resource(&app, origin_a, resource_a);
    // The active canvas is B, but A's loaded authority still projects A's graph.
    assert!(
        app.behavior_execution_origin().is_none(),
        "execution is refused while its legacy lane names B"
    );
    let entries = crate::behaviors::entries_since(&app, cursor);
    assert_eq!(entries.len(), 1);
    let surface = uuid::Uuid::from_u128(7).to_string();
    let folder_a = uuid::Uuid::from_u128(10).to_string();
    let folder_b = uuid::Uuid::from_u128(11).to_string();
    assert!(
        entries[0]
            .scopes
            .iter()
            .any(|scope| scope.segments() == &[folder_a.clone(), surface.clone()])
    );
    assert!(
        !entries[0]
            .scopes
            .iter()
            .any(|scope| scope.segments().contains(&folder_b))
    );
    assert!(!entries[0].scopes.iter().any(|scope| {
        scope
            .segments()
            .contains(&uuid::Uuid::from_u128(8).to_string())
    }));
    let subject = Subject::new([3; 32]);
    for scope in [
        ScopePath::root(),
        ScopePath::parse(&format!("{folder_a}/{surface}")).unwrap(),
    ] {
        let watch = Watch {
            subject,
            scope,
            self_author: subject.to_hex(),
            cursor,
        };
        assert!(watch.matches(&entries[0].as_event()));
        assert!(
            foreign
                .iter()
                .all(|entry| !watch.matches(&entry.as_event()))
        );
    }
    app.graph_runtimes.activate(graph_a);
    assert_eq!(app.behavior_execution_origin(), Some(origin_a));
}

#[test]
fn same_graph_reload_excludes_old_generation_even_with_equal_ids() {
    let mut app = App::test_stub();
    let (graph, resource) = paired_graph(false, 10);
    app.graph_runtimes.set_graph(graph.clone());
    app.bind_behavior_journal().unwrap();
    let old = app.behavior_origin().unwrap();
    record_resource(&app, old, resource);
    app.graph_runtimes.set_graph(graph);
    assert!(
        app.behavior_origin().is_none(),
        "replacement invalidates loaded authority until explicit adoption"
    );
    app.bind_behavior_journal().unwrap();
    let new = app.behavior_origin().unwrap();
    assert_ne!(old, new);
    assert!(crate::behaviors::entries_since(&app, 0).is_empty());
    record_resource(&app, new, resource);
    assert_eq!(crate::behaviors::entries_since(&app, 0).len(), 1);
}

#[test]
fn restored_graph_watch_cursor_cannot_hide_first_new_edit() {
    let mut app = App::test_stub();
    let subject = Subject::new([4; 32]);
    let line = format!("{} 900 {} /", subject.to_hex(), subject.to_hex());
    app.watches = servitor::WatchTable::from_wire_lines([line.as_str()]).unwrap();
    app.bind_behavior_journal().unwrap();
    let origin = app.behavior_origin().unwrap();
    assert_eq!(app.behavior_cursor, 900);
    let seq = app
        .journal
        .lock()
        .unwrap()
        .record_as(
            origin,
            Author::user(),
            CapturedDelta::ReplayRemoveNodeById {
                node_id: uuid::Uuid::from_u128(7).to_string(),
            },
        )
        .unwrap();
    assert_eq!(seq, 901);
    let entry = &crate::behaviors::entries_since(&app, app.behavior_cursor)[0];
    assert!(app.watches.watches()[0].matches(&entry.as_event()));
}

#[test]
fn exhausted_saved_cursor_refuses_capture_binding_and_allows_fresh_retry() {
    let mut app = App::test_stub();
    let subject = Subject::new([4; 32]);
    let line = format!("{} {} user /", subject.to_hex(), u64::MAX);
    app.watches = servitor::WatchTable::from_wire_lines([line.as_str()]).unwrap();
    assert!(
        app.bind_behavior_journal()
            .unwrap_err()
            .contains("exhausted")
    );
    assert!(app.behavior_origin().is_none());
    app.watches = servitor::WatchTable::new();
    app.bind_behavior_journal().unwrap();
    assert!(app.behavior_origin().is_some());
}

#[test]
fn sample_canvas_replacement_rebinds_once_and_retains_display_budget() {
    let mut app = App::test_stub();
    let old = app.behavior_origin().unwrap();
    app.graph_runtimes.set_physics_display_rate(Some(144_000));
    app.graph_runtimes
        .replace_active_canvas(mere::canvas::Canvas::with_sample_graph());
    assert!(app.behavior_origin().is_none());
    app.bind_behavior_journal().unwrap();
    let new = app.behavior_origin().unwrap();
    assert_ne!(old, new);
    let member = app.graph_runtimes.graph().nodes().next().unwrap().1.id;
    let count = app.journal.lock().unwrap().entries().len();
    app.graph_runtimes
        .set_node_title_for(member, "one edit".into());
    let journal = app.journal.lock().unwrap();
    assert_eq!(journal.entries().len(), count + 1);
    assert_eq!(journal.entries().last().unwrap().origin, new);
}

#[test]
fn scratch_fork_assembly_cannot_record_into_live_stream() {
    let mut app = App::test_stub();
    let key = app.graph_runtimes.visit("https://scratch.test/");
    let member = app.graph_runtimes.graph().get_node(key).unwrap().id;
    let mut scratch = app.graph_runtimes.graph().clone();
    assert!(!scratch.is_recording());
    let count = app.journal.lock().unwrap().entries().len();
    mere::kernel::graph::apply::apply_graph_delta(
        &mut scratch,
        mere::kernel::graph::apply::GraphDelta::ReplaySetNodeTitleById {
            node_id: member,
            title: "fork edit".into(),
        },
    );
    assert_eq!(app.journal.lock().unwrap().entries().len(), count);
}

#[test]
fn loaded_new_watch_table_survives_old_cascade_restore_attempt() {
    let mut app = App::test_stub();
    let old = app.behavior_origin().unwrap();
    let old_table = std::mem::take(&mut app.watches);
    app.graph_runtimes.set_graph(Graph::new());
    let subject = Subject::new([5; 32]);
    let line = format!("{} 17 user /", subject.to_hex());
    app.watches = servitor::WatchTable::from_wire_lines([line.as_str()]).unwrap();
    app.bind_behavior_journal().unwrap();
    let current = app.behavior_execution_origin();
    assert!(!restore_table(&mut app.watches, old_table, old, current));
    assert_eq!(app.watches.watches().len(), 1);
    assert_eq!(app.watches.watches()[0].subject, subject);
}

#[cfg(feature = "piccolo")]
#[test]
fn foreign_only_tail_advances_without_waking_authorized_local_body() {
    use crate::action::Action;
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub();
    app.data_root = root.path().to_path_buf();
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = root.path().join("origin-watch.lua");
    std::fs::write(
        &pack,
        "-- @watch https://shared.test/page\nmere.open('mere://origin-woke')",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    app.update(Action::ReseedLayout);
    assert_eq!(app.watches.watches().len(), 1);
    let count_before_foreign = app.graph_runtimes.graph().node_count();
    let origin_a = app.behavior_origin().unwrap();
    let watched = app
        .graph_runtimes
        .graph()
        .get_node_by_url("https://shared.test/page")
        .unwrap()
        .0;
    let resource = app
        .graph_runtimes
        .graph()
        .shown_resource_id(watched)
        .unwrap();
    let graph_b = GraphId::from_uuid(uuid::Uuid::from_u128(22));
    let copy = app.graph_runtimes.graph().clone();
    app.graph_runtimes
        .activate_or_insert(graph_b, None, mere::canvas::Canvas::with_graph(copy));
    let origin_b = app.graph_runtimes.get(graph_b).unwrap().origin().unwrap();
    app.graph_runtimes.activate(origin_a.graph);
    app.take_events();
    record_resource(&app, origin_b, resource);
    let boundary = app.journal.lock().unwrap().high_water();
    crate::behaviors::drain(&mut app);
    assert_eq!(app.behavior_cursor, boundary);
    assert_eq!(
        app.graph_runtimes.graph().node_count(),
        count_before_foreign
    );
    assert!(
        !app.take_events()
            .iter()
            .any(|event| matches!(event, crate::observe::AppEvent::DenizenRan(_)))
    );
    record_resource(&app, origin_a, resource);
    crate::behaviors::drain(&mut app);
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://origin-woke")
            .is_some()
    );
    assert!(
        app.take_events()
            .iter()
            .any(|event| matches!(event, crate::observe::AppEvent::DenizenRan(_)))
    );
}
