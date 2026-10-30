// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

fn fixture() -> (tempfile::TempDir, App, crate::panes::GraphId, uuid::Uuid) {
    let root = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(root.path().to_path_buf());
    let custody = crate::trail_memory::custody_test_fixture::custody();
    app.research.set_available(true);
    app.research.access(Some(crate::research::ClipAccess {
        root: root.path().into(),
        sealer: custody.sealer,
        epoch: custody.epoch,
    }));
    app.sync_research_context();
    let graph = app.graph_runtimes.active_graph();
    let node = app.graph_runtimes.visit("https://fixture.test/entity");
    let entity = app.graph_runtimes.graph().shown_resource_id(node).unwrap();
    (root, app, graph, entity)
}

pub(super) fn smallest_scope_limit(app: &mut App, work: bool) -> usize {
    let mut low = 0;
    let mut high = if work {
        app.research_search.preferences.records.max_work
    } else {
        app.research_search.preferences.records.max_input_bytes
    };
    while low < high {
        let mid = low + (high - low) / 2;
        if work {
            app.research_search.preferences.records.max_work = mid;
        } else {
            app.research_search.preferences.records.max_input_bytes = mid;
        }
        if app.structured_scope().is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    if work {
        app.research_search.preferences.records.max_work = low;
    } else {
        app.research_search.preferences.records.max_input_bytes = low;
    }
    low
}

#[test]
fn model_free_attachment_preflight_preserves_work_and_metadata_boundaries() {
    for result in [false, true] {
        for work in [false, true] {
            let (_root, mut app, graph, entity) = fixture();
            let author = app.structured_author_identity().unwrap().asserter_iri();
            let manifest = eidetic::ManifestId::of_blob(b"candidate");
            let (reserve_work, reserve_bytes) =
                crate::research_record_graph::attachment_reservation(result, &author, manifest);
            let minimum = smallest_scope_limit(&mut app, work);
            if work {
                app.research_search.preferences.records.max_work = minimum + reserve_work - 1;
            } else {
                app.research_search.preferences.records.max_input_bytes =
                    minimum + reserve_bytes - 1;
            }
            let before = app.graph_runtimes.graph().revision();
            assert!(app.structured_scope().is_ok());
            assert!(
                app.structured_attach_binding(graph, entity, manifest, result, &author)
                    .is_err()
            );
            assert_eq!(app.graph_runtimes.graph().revision(), before);
            if work {
                app.research_search.preferences.records.max_work = minimum + reserve_work;
            } else {
                app.research_search.preferences.records.max_input_bytes = minimum + reserve_bytes;
            }
            app.structured_attach_binding(graph, entity, manifest, result, &author)
                .unwrap();
            assert!(app.structured_scope().is_ok());
        }
    }
}

#[test]
fn model_free_recovery_inventory_available_after_unrelated_graph_growth() {
    let (_root, mut app, graph, _entity) = fixture();
    let minimum = smallest_scope_limit(&mut app, true);
    app.research_search.preferences.records.max_work = minimum;
    assert!(app.structured_scope().is_ok());
    app.graph_runtimes
        .get_mut(graph)
        .unwrap()
        .canvas
        .ingest_graph(|graph| {
            mere::kernel::graph::apply::add_node(
                graph,
                Some(uuid::Uuid::new_v4()),
                "https://fixture.test/later".into(),
                Default::default(),
            );
            true
        });
    assert!(app.structured_scope().is_err());
    let effects = app.structured_refresh_recovery();
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ResearchRecords(Job::Recovery { .. })))
    );
    assert!(app.research_search.structured.controller.error.is_none());
}
