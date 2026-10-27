// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::forme_workspace::{
    GraphshellFormeSession as FormeSession, GraphshellPreparedSave as PreparedSave,
    GraphshellSaveKind as SaveKind,
};
use muniment::Backend;
use platen::workspace::{FormeLoadGate, FormeWriteFence};
use uuid::Uuid;

trait PersistPreparedSave {
    async fn save<B: Backend>(&self, backend: B) -> Result<(), String>;
}
impl PersistPreparedSave for PreparedSave {
    async fn save<B: Backend>(&self, backend: B) -> Result<(), String> {
        let bytes = self.bytes()?;
        backend
            .put(&crate::forme_workspace::slot(self.graph_id()), &bytes)
            .await
            .map_err(|e| e.to_string())
    }
}
async fn load<B: Backend>(
    backend: &B,
    graph_id: Uuid,
    graph: &mere::kernel::graph::Graph,
) -> Result<FormeSession, String> {
    let bytes = backend
        .get(&crate::forme_workspace::slot(graph_id))
        .await
        .map_err(|e| e.to_string())?;
    FormeSession::from_bytes(bytes.as_deref(), graph_id, graph)
}
use crate::{
    app::GraphshellApp,
    mere_host::{FIXTURE_PERSONA_ADDRESS, FIXTURE_WEB_ADDRESS, GRAPHSHELL, SelectedPersonaRef},
    product::{ExportRequest, TransferScope},
};
use mere::canvas::Canvas;
use muniment::MemoryBackend;

type App = GraphshellApp<MemoryBackend>;
fn fixture() -> (MemoryBackend, App, FormeSession, Uuid) {
    let backend = MemoryBackend::new();
    let app = App::fixture(
        backend.clone(),
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.into(),
            profile: "profile:forme-lifecycle".into(),
        },
    )
    .unwrap();
    let owner = app.host.graph_session().id().0;
    let member = app
        .host
        .graph()
        .get_node_by_url(FIXTURE_WEB_ADDRESS)
        .unwrap()
        .1
        .id;
    let mut workspace = FormeWorkspace::new(owner);
    workspace.open(member, app.host.graph()).unwrap();
    (backend, app, FormeSession::new(workspace), member)
}
fn model_fixture() -> (mere::kernel::graph::Graph, FormeSession, [Uuid; 3]) {
    use mere::kernel::graph::apply::add_node;
    let mut graph = mere::kernel::graph::Graph::new();
    let members = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
    for member in members {
        add_node(
            &mut graph,
            Some(member),
            "https://example.test/shared".into(),
            Default::default(),
        );
    }
    let owner = Uuid::from_u128(10);
    let mut workspace = FormeWorkspace::new(owner);
    workspace.open(members[0], &graph).unwrap();
    (graph, FormeSession::new(workspace), members)
}
fn codicil(app: &App, member: Uuid) -> Vec<u8> {
    app.host
        .export_product_codicil(ExportRequest {
            focused: member,
            selected: vec![member],
            scope: TransferScope::ObjectOnly,
            exported_at_ms: 12,
            include_local_file_locations: false,
            scene: None,
        })
        .unwrap()
}
async fn persist(session: &mut FormeSession, backend: MemoryBackend) {
    let proposal = session.prepare_save(SaveKind::Visibility, 17).unwrap();
    let result = proposal.save(backend).await;
    session.finish_save(proposal, result).unwrap();
}

async fn save_kind(session: &mut FormeSession, backend: MemoryBackend, kind: SaveKind) {
    let proposal = session.prepare_save(kind, 1000).unwrap();
    let result = proposal.save(backend).await;
    session.finish_save(proposal, result).unwrap();
}

#[test]
fn inspector_binding_persists_without_committing_or_discarding_layout_draft() {
    pollster::block_on(async {
        use crate::pane_source::InspectorSource;
        let (graph, mut session, [a, _, c]) = model_fixture();
        let backend = MemoryBackend::new();
        let owner = session.committed().document.graph_id;
        persist(&mut session, backend.clone()).await;
        let document = serde_json::to_value(&session.committed().document).unwrap();
        let original_created = session.committed().document.created_at_ms;
        let original_updated = session.committed().document.updated_at_ms;
        session.unlock().unwrap();
        session.edit(None, 1, |w| w.open(c, &graph)).unwrap();
        let update = session.preferences_update(false, |preferences| {
            preferences.inspector_source = InspectorSource::Pinned(a);
        });
        save_kind(&mut session, backend.clone(), update).await;
        assert!(session.editing());
        assert!(session.view().members().contains(&c));
        assert!(!session.committed().members().contains(&c));
        assert_eq!(
            serde_json::to_value(&session.committed().document).unwrap(),
            document
        );
        assert_eq!(
            session.view().preferences.inspector_source,
            InspectorSource::Pinned(a)
        );
        assert_eq!(session.committed().preferences.focused_reading, Some(a));
        assert_eq!(session.view().preferences.focused_reading, Some(c));
        assert_ne!(
            session.committed().preferences.reading_trail,
            session.view().preferences.reading_trail,
            "an inspector update must preserve the separate committed and draft trails"
        );
        assert_eq!(session.committed().document.created_at_ms, original_created);
        assert_eq!(session.committed().document.updated_at_ms, original_updated);
        session.discard().unwrap();
        let cold = load(&backend, owner, &graph).await.unwrap();
        assert_eq!(
            cold.view().preferences.inspector_source,
            InspectorSource::Pinned(a)
        );
        assert!(!cold.view().members().contains(&c));
        assert_eq!(
            cold.view().preferences.focused_reading,
            session.committed().preferences.focused_reading
        );
        let other = load(&backend, Uuid::from_u128(99), &graph).await.unwrap();
        assert_eq!(
            other.view().preferences.inspector_source,
            InspectorSource::Active
        );
        assert!(other.view().preferences.focused_reading.is_none());
    });
}

#[test]
fn preference_retry_rejects_stale_draft_payload_but_accepts_unchanged_retry() {
    use crate::pane_source::InspectorSource;
    let (graph, mut session, [a, b, c]) = model_fixture();
    session.unlock().unwrap();
    session
        .edit(None, 1, |workspace| workspace.open(c, &graph))
        .unwrap();

    let unchanged = session.preferences_update(false, |preferences| {
        preferences.inspector_source = InspectorSource::Pinned(a);
    });
    let failed = session.prepare_save(unchanged.clone(), 2).unwrap();
    assert!(session.finish_save(failed, Err("offline".into())).is_err());
    let retry = session.prepare_save(unchanged, 3).unwrap();
    session.finish_save(retry, Ok(())).unwrap();
    assert_eq!(session.committed().preferences.focused_reading, Some(a));
    assert_eq!(session.view().preferences.focused_reading, Some(c));

    let stale = session.preferences_update(false, |preferences| {
        preferences.inspector_source = InspectorSource::Pinned(b);
    });
    let failed = session.prepare_save(stale.clone(), 4).unwrap();
    assert!(session.finish_save(failed, Err("offline".into())).is_err());
    session
        .edit(None, 5, |workspace| workspace.open(b, &graph))
        .unwrap();
    assert!(session.prepare_save(stale, 6).is_err());
    assert_eq!(
        session.committed().preferences.inspector_source,
        InspectorSource::Pinned(a)
    );
    assert_eq!(session.committed().preferences.focused_reading, Some(a));
    assert_eq!(
        session.view().preferences.inspector_source,
        InspectorSource::Pinned(a)
    );
    assert_eq!(session.view().preferences.focused_reading, Some(b));

    let fresh = session.preferences_update(false, |preferences| {
        preferences.inspector_source = InspectorSource::Pinned(b);
    });
    let saved = session.prepare_save(fresh, 7).unwrap();
    session.finish_save(saved, Ok(())).unwrap();
    assert_eq!(
        session.committed().preferences.inspector_source,
        InspectorSource::Pinned(b)
    );
    assert_eq!(session.committed().preferences.focused_reading, Some(a));
    assert_eq!(session.view().preferences.focused_reading, Some(b));
}

#[test]
fn reading_preferences_persist_without_layout_history_and_failed_write_is_atomic() {
    pollster::block_on(async {
        let (graph, mut session, [a, b, c]) = model_fixture();
        let owner = session.view().document.graph_id;
        let backend = MemoryBackend::new();
        let mut value = tabard::ReadingPreferences::default();
        value.step_text(true);
        value.step_column(false);
        session.unlock().unwrap();
        session
            .edit(None, 1, |workspace| workspace.open(c, &graph))
            .unwrap();
        let layout = session.view().geometry.clone();
        let update_value = value.clone();
        let update = session.preferences_update(true, move |preferences| {
            preferences.reading_preferences.set(a, update_value.clone());
        });
        save_kind(&mut session, backend.clone(), update).await;
        assert!(session.editing());
        assert_eq!(session.view().geometry, layout);
        assert_eq!(session.view().preferences.focused_reading, Some(c));
        assert_eq!(
            session.view().preferences.reading_preferences.get(&a),
            value
        );
        session.discard().unwrap();
        let mut cold = load(&backend, owner, &graph).await.unwrap();
        assert_eq!(cold.view().preferences.reading_preferences.get(&a), value);
        assert!(!cold.view().preferences.reading_preferences.contains(&b));
        assert_eq!(cold.view().preferences.focused_reading, Some(a));
        assert!(!cold.can_undo(), "preferences are not layout gestures");
        let committed = serde_json::to_vec(cold.committed()).unwrap();
        let mut next = value;
        next.step_text(true);
        let update = cold.preferences_update(true, move |preferences| {
            preferences.reading_preferences.set(a, next.clone());
        });
        let proposal = cold.prepare_save(update, 2).unwrap();
        assert!(cold.finish_save(proposal, Err("offline".into())).is_err());
        assert_eq!(serde_json::to_vec(cold.committed()).unwrap(), committed);
    });
}

#[test]
fn anchored_reading_visits_reopen_and_failed_progress_keeps_acknowledged_state() {
    pollster::block_on(async {
        let (graph, mut session, [a, b, c]) = model_fixture();
        let owner = session.view().document.graph_id;
        let backend = MemoryBackend::new();
        let anchor = tabard::ReadingAnchor {
            source: "https://example.test/a".into(),
            revision: 42,
            text: 7,
            offset: 0,
            exact: "A retained passage".into(),
            prefix: String::new(),
            suffix: String::new(),
            line_offset: 0.0,
            fallback_y: 600.0,
        };
        let mut trail = tabard::ReadingTrail::default();
        trail.visit(a);
        trail.anchor_current(anchor.clone());
        trail.visit(b);
        session.unlock().unwrap();
        session
            .edit(None, 1, |workspace| workspace.open(c, &graph))
            .unwrap();
        let layout = session.view().geometry.clone();
        let update_anchor = anchor.clone();
        let update_trail = trail.clone();
        let update = session.preferences_update(true, move |preferences| {
            preferences.reading_anchors.insert(a, update_anchor.clone());
            preferences.reading_trail = update_trail.clone();
        });
        save_kind(&mut session, backend.clone(), update).await;
        assert_eq!(session.view().geometry, layout);
        assert!(session.editing());
        session.discard().unwrap();
        let mut cold = load(&backend, owner, &graph).await.unwrap();
        assert_eq!(
            cold.view().preferences.reading_anchors.get(&a),
            Some(&anchor)
        );
        let mut reopened = cold.view().preferences.reading_trail.clone();
        assert_eq!(reopened.step(false), Some(a));
        assert_eq!(reopened.current_anchor(), Some(&anchor));
        let committed = serde_json::to_vec(cold.committed()).unwrap();
        let update = cold.preferences_update(true, |preferences| {
            preferences.reading_anchors.clear();
            preferences.reading_trail = Default::default();
        });
        let proposal = cold.prepare_save(update, 3).unwrap();
        assert!(cold.finish_save(proposal, Err("offline".into())).is_err());
        assert_eq!(serde_json::to_vec(cold.committed()).unwrap(), committed);
    });
}

#[test]
fn failed_source_and_scope_writes_keep_acknowledged_policy_and_reject_foreign_graph() {
    use crate::pane_source::InspectorSource;
    use crate::scope_policy::*;
    let (_, mut session, [a, _, _]) = model_fixture();
    let owner = session.committed().document.graph_id;
    let update = session.preferences_update(false, |preferences| {
        preferences.inspector_source = InspectorSource::Pinned(a);
    });
    let proposal = session.prepare_save(update, 1).unwrap();
    assert!(
        session
            .finish_save(proposal, Err("offline".into()))
            .is_err()
    );
    assert_eq!(
        session.committed().preferences.inspector_source,
        InspectorSource::Active
    );

    let mut navigation = session.committed().preferences.scope_navigation.clone();
    let anchor = tabard::ReadingAnchor {
        source: "mere://reading/a".into(),
        revision: 42,
        text: 3,
        offset: 0,
        exact: "A retained passage".into(),
        prefix: String::new(),
        suffix: String::new(),
        line_offset: 0.0,
        fallback_y: 600.0,
    };
    let place = ScopePlace {
        contexts: Some(vec![
            ScopeContext {
                level: ScopeLevel::Graph,
                graph: owner,
                root: None,
            },
            ScopeContext {
                level: ScopeLevel::Neighborhood,
                graph: owner,
                root: Some(a),
            },
        ]),
        level: ScopeLevel::Document,
        graph: owner,
        root: Some(a),
        focus: Some(a),
    };
    navigation.visit(ScopeVisit {
        place: place.clone(),
        camera: None,
        anchor: Some(anchor.clone()),
    });
    navigation.visit(ScopeVisit {
        place: place.up().unwrap(),
        camera: Some(ScopeCamera {
            x: 100.0,
            y: 80.0,
            zoom: 2.0,
        }),
        anchor: None,
    });
    let update = session.preferences_update(false, {
        let navigation = navigation.clone();
        move |preferences| preferences.scope_navigation = navigation.clone()
    });
    let proposal = session.prepare_save(update, 2).unwrap();
    assert!(
        session
            .finish_save(proposal, Err("offline".into()))
            .is_err()
    );
    assert!(
        session
            .committed()
            .preferences
            .scope_navigation
            .current()
            .is_none()
    );
    let mut foreign = navigation.current().unwrap().clone();
    foreign.place.graph = Uuid::nil();
    navigation.visit(foreign);
    let update = session.preferences_update(false, move |preferences| {
        preferences.scope_navigation = navigation.clone();
    });
    assert!(session.prepare_save(update, 3).is_err());
}

#[test]
fn closing_a_reading_prefers_its_stack_before_other_tiles() {
    use mere::kernel::graph::apply::add_node;
    let (graph, mut session, [a, b, _]) = model_fixture();
    let c = Uuid::from_u128(99);
    let mut graph = graph;
    add_node(
        &mut graph,
        Some(c),
        "https://example.test/third".into(),
        Default::default(),
    );
    session.unlock().unwrap();
    session
        .edit(None, 1, |workspace| {
            workspace.read(b, &graph)?;
            workspace.open(c, &graph)
        })
        .unwrap();
    session
        .edit(None, 2, |workspace| {
            workspace.event(
                workbench::TileEvent::Activated(workspace.tile_for_member(b).unwrap()),
                &graph,
            )?;
            workspace.event(
                workbench::TileEvent::Closed(workspace.tile_for_member(b).unwrap()),
                &graph,
            )
        })
        .unwrap();
    assert_eq!(session.view().preferences.focused_reading, Some(a));
    assert!(session.view().members().contains(&c));
    session
        .edit(None, 3, |workspace| {
            workspace.event(
                workbench::TileEvent::Closed(workspace.tile_for_member(a).unwrap()),
                &graph,
            )?;
            workspace.event(
                workbench::TileEvent::Closed(workspace.tile_for_member(c).unwrap()),
                &graph,
            )
        })
        .unwrap();
    assert_eq!(session.view().preferences.focused_reading, None);
    assert!(session.view().members().is_empty());
    session
        .edit(None, 4, |workspace| workspace.open(b, &graph))
        .unwrap();
    assert_eq!(session.view().preferences.focused_reading, Some(b));
}

#[test]
fn immutable_scope_contexts_are_validated_against_the_owner_graph() {
    use crate::scope_policy::*;
    let (graph, session, _) = model_fixture();
    let owner = session.committed().document.graph_id;
    let mut preferences = session.committed().preferences.clone();
    preferences.scope_navigation.visit(ScopeVisit {
        place: ScopePlace {
            level: ScopeLevel::Graph,
            graph: owner,
            root: None,
            focus: None,
            contexts: None,
        },
        camera: None,
        anchor: None,
    });
    let mut wire = serde_json::to_value(&preferences.scope_navigation).unwrap();
    wire["lineage"]["visits"][0]["context"]["place"]["graph"] =
        serde_json::json!(Uuid::from_u128(20));
    preferences.scope_navigation = serde_json::from_value(wire).unwrap();
    let mut workspace = FormeWorkspace::new(owner);
    workspace.preferences = preferences;
    assert!(workspace.validate(owner).is_err());
    assert!(FormeSession::from_bytes(None, owner, &graph).is_ok());
}

#[test]
fn scope_route_and_anchor_persist_while_layout_draft_and_timestamp_policy_hold() {
    pollster::block_on(async {
        use crate::scope_policy::*;
        let (graph, mut session, [a, _, c]) = model_fixture();
        let owner = session.committed().document.graph_id;
        let backend = MemoryBackend::new();
        persist(&mut session, backend.clone()).await;
        let timestamp = session.committed().document.updated_at_ms;
        session.unlock().unwrap();
        session
            .edit(None, 1, |workspace| workspace.open(c, &graph))
            .unwrap();

        let anchor = tabard::ReadingAnchor {
            source: "mere://scope/document".into(),
            revision: 11,
            text: 2,
            offset: 0,
            exact: "Scoped passage".into(),
            prefix: String::new(),
            suffix: String::new(),
            line_offset: 0.0,
            fallback_y: 240.0,
        };
        let place = ScopePlace {
            contexts: Some(vec![
                ScopeContext {
                    level: ScopeLevel::Graph,
                    graph: owner,
                    root: None,
                },
                ScopeContext {
                    level: ScopeLevel::Neighborhood,
                    graph: owner,
                    root: Some(a),
                },
            ]),
            level: ScopeLevel::Document,
            graph: owner,
            root: Some(a),
            focus: Some(a),
        };
        let place_for_update = place.clone();
        let anchor_for_update = anchor.clone();
        let update = session.preferences_update(false, move |preferences| {
            preferences.scope_navigation.visit(ScopeVisit {
                place: place_for_update.clone(),
                camera: Some(ScopeCamera {
                    x: 24.0,
                    y: 32.0,
                    zoom: 1.5,
                }),
                anchor: Some(anchor_for_update.clone()),
            });
        });
        save_kind(&mut session, backend.clone(), update).await;
        assert!(session.editing());
        assert!(session.view().members().contains(&c));
        assert_eq!(session.committed().document.updated_at_ms, timestamp);
        session.discard().unwrap();

        let reopened = load(&backend, owner, &graph).await.unwrap();
        let visit = reopened
            .view()
            .preferences
            .scope_navigation
            .current()
            .unwrap();
        assert_eq!(visit.place, place);
        assert_eq!(visit.anchor, Some(anchor));
        assert_eq!(
            visit.camera,
            Some(ScopeCamera {
                x: 24.0,
                y: 32.0,
                zoom: 1.5
            })
        );
        assert!(!reopened.view().members().contains(&c));
        assert_eq!(reopened.committed().document.updated_at_ms, timestamp);
    });
}

#[test]
fn owner_lifecycle_same_app_undo_redo_preserves_layout_and_masks_only_missing_cells() {
    let (_, mut app, mut session, _) = fixture();
    let owner = app.host.graph_session().id().0;
    let added =
        crate::local_intake::create_address(&mut app, "https://example.test/forme-new", "New")
            .unwrap();
    session.unlock().unwrap();
    session
        .edit(None, 1, |w| w.open(added, app.host.graph()))
        .unwrap();
    let before = serde_json::to_vec(session.view()).unwrap();
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas
        .set_forme_region(
            crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                .unwrap(),
        )
        .unwrap();
    assert!(app.host.undo_now(GRAPHSHELL).unwrap().is_some());
    assert_eq!(app.host.graph_session().id().0, owner);
    assert!(app.host.graph().get_node_by_id(added).is_none());
    canvas.set_graph(app.host.graph().clone());
    assert!(canvas.forme_region().is_none());
    canvas
        .set_forme_region(
            crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                .unwrap(),
        )
        .unwrap();
    assert!(
        canvas
            .forme_region()
            .is_none_or(|region| !region.cells.iter().any(|c| c.member == added))
    );
    assert_eq!(serde_json::to_vec(session.view()).unwrap(), before);
    assert!(session.can_undo(), "draft history survives graph undo");
    assert!(app.host.redo_now(GRAPHSHELL).unwrap().is_some());
    canvas.set_graph(app.host.graph().clone());
    canvas
        .set_forme_region(
            crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                .unwrap(),
        )
        .unwrap();
    assert!(
        canvas
            .forme_region()
            .unwrap()
            .cells
            .iter()
            .any(|c| c.member == added)
    );
    assert_eq!(serde_json::to_vec(session.view()).unwrap(), before);
}

#[test]
fn owner_lifecycle_actual_codicil_new_app_owner_has_empty_model_and_preserves_old_slot() {
    pollster::block_on(async {
        let (backend, mut app, mut old, member) = fixture();
        let old_owner = app.host.graph_session().id().0;
        persist(&mut old, backend.clone()).await;
        let old_bytes = backend.get(&slot(old_owner)).await.unwrap().unwrap();
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        let mut invalid_scene = canvas
            .capture_scene(
                "Graphshell working scene",
                mere::canvas::physics_controls::FREE_ARRANGEMENT,
                "",
            )
            .unwrap();
        invalid_scene.camera_zoom = f32::NAN;
        let bytes = codicil(&app, member);
        app.host.replace_with_product_codicil(&bytes).unwrap();
        assert!(
            canvas
                .restore_scene(app.host.graph(), &invalid_scene, (900, 600))
                .is_err(),
            "restore error follows accepted authority mutation"
        );
        let owner = app.host.graph_session().id().0;
        assert_ne!(owner, old_owner);
        let mut gate = FormeLoadGate::default();
        let ticket = gate.request(owner);
        let loaded = load(&backend, owner, app.host.graph()).await.unwrap();
        assert!(gate.finish_loaded(ticket, Some(app.host.graph_session().id().0), &loaded));
        assert_eq!(loaded.committed().document.graph_id, owner);
        assert!(
            loaded.view().members().is_empty(),
            "overlapping member UUID is not ownership"
        );
        assert!(
            crate::forme_workspace::project_region(loaded.view(), owner, app.host.graph())
                .unwrap()
                .is_none()
        );
        assert!(
            crate::forme_workspace::project_region(old.view(), owner, app.host.graph()).is_err()
        );
        assert_eq!(
            backend.get(&slot(old_owner)).await.unwrap().unwrap(),
            old_bytes
        );
        assert!(
            backend.get(&slot(owner)).await.unwrap().is_none(),
            "loading does not copy/write old geometry"
        );
    });
}

#[test]
fn owner_lifecycle_failed_codicil_preserves_actual_owner_layout_and_slot() {
    pollster::block_on(async {
        let (backend, mut app, mut session, _) = fixture();
        let owner = app.host.graph_session().id().0;
        persist(&mut session, backend.clone()).await;
        let bytes = backend.get(&slot(owner)).await.unwrap().unwrap();
        let view = serde_json::to_vec(session.view()).unwrap();
        assert!(
            app.host
                .replace_with_product_codicil(b"not a codicil")
                .is_err()
        );
        assert_eq!(app.host.graph_session().id().0, owner);
        assert_eq!(serde_json::to_vec(session.view()).unwrap(), view);
        assert!(
            crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                .unwrap()
                .is_some()
        );
        assert_eq!(backend.get(&slot(owner)).await.unwrap().unwrap(), bytes);
    });
}

#[test]
fn owner_lifecycle_loaded_session_rejects_absent_foreign_and_repeated_owner_stale_ticket() {
    pollster::block_on(async {
        let (backend, mut app, mut old, member) = fixture();
        let a = app.host.graph_session().id().0;
        persist(&mut old, backend.clone()).await;
        let mut gate = FormeLoadGate::default();
        let old_ticket = gate.request(a);
        let a_loaded = load(&backend, a, app.host.graph()).await.unwrap();
        assert!(!gate.finish_loaded(old_ticket, None, &a_loaded));
        let bytes = codicil(&app, member);
        app.host.replace_with_product_codicil(&bytes).unwrap();
        let b = app.host.graph_session().id().0;
        let b_ticket = gate.request(b);
        let b_loaded = load(&backend, b, app.host.graph()).await.unwrap();
        assert!(!gate.finish_loaded(old_ticket, Some(b), &a_loaded));
        assert!(!gate.finish_loaded(b_ticket, Some(b), &a_loaded));
        let newest = gate.request(b);
        assert!(
            !gate.finish_loaded(b_ticket, Some(b), &b_loaded),
            "same-owner newer request retires stale completion"
        );
        assert!(gate.finish_loaded(newest, Some(app.host.graph_session().id().0), &b_loaded));
        assert!(!gate.finish_loaded(newest, Some(b), &b_loaded));
    });
}

#[test]
fn owner_lifecycle_old_save_cannot_mutate_reloaded_same_slot_busy_session() {
    pollster::block_on(async {
        let (backend, app, mut old, _) = fixture();
        let owner = app.host.graph_session().id().0;
        persist(&mut old, backend.clone()).await;
        let old_proposal = old.prepare_save(SaveKind::Visibility, 20).unwrap();
        let mut current = load(&backend, owner, app.host.graph()).await.unwrap();
        current.unlock().unwrap();
        current
            .edit(None, 21, |w| {
                w.bounds[0] -= 10.;
                Ok(())
            })
            .unwrap();
        let current_proposal = current.prepare_save(SaveKind::Apply, 22).unwrap();
        let before = serde_json::to_vec(current.view()).unwrap();
        let committed = serde_json::to_vec(current.committed()).unwrap();
        let result = old_proposal.save(backend.clone()).await;
        assert!(current.finish_save(old_proposal, result).is_err());
        assert!(current.busy());
        assert!(current.editing());
        assert_eq!(serde_json::to_vec(current.view()).unwrap(), before);
        assert_eq!(serde_json::to_vec(current.committed()).unwrap(), committed);
        let duplicate = current_proposal.clone();
        let result = current_proposal.save(backend).await;
        current.finish_save(current_proposal, result).unwrap();
        assert!(!current.busy());
        assert!(current.finish_save(duplicate, Ok(())).is_err());
    });
}

#[test]
fn owner_lifecycle_valid_failed_save_releases_own_request_and_retains_draft_for_retry() {
    pollster::block_on(async {
        let (backend, _, mut session, _) = fixture();
        session.unlock().unwrap();
        session
            .edit(None, 1, |w| {
                w.bounds[1] -= 20.;
                Ok(())
            })
            .unwrap();
        let before = serde_json::to_vec(session.view()).unwrap();
        let proposal = session.prepare_save(SaveKind::Apply, 2).unwrap();
        let duplicate = proposal.clone();
        assert!(
            session
                .finish_save(proposal, Err("controlled failed acknowledgement".into()))
                .is_err()
        );
        assert!(!session.busy());
        assert!(session.editing());
        assert_eq!(serde_json::to_vec(session.view()).unwrap(), before);
        assert!(session.finish_save(duplicate, Ok(())).is_err());
        let retry = session.prepare_save(SaveKind::Apply, 3).unwrap();
        let result = retry.save(backend).await;
        session.finish_save(retry, result).unwrap();
        assert!(!session.editing());
    });
}

#[derive(Clone)]
struct DelayedPut {
    inner: MemoryBackend,
    started: std::sync::Arc<std::sync::atomic::AtomicBool>,
    released: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Backend for DelayedPut {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, muniment::StoreError> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), muniment::StoreError> {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        std::future::poll_fn(|_| {
            if self.released.load(std::sync::atomic::Ordering::SeqCst) {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        })
        .await;
        self.inner.put(key, bytes).await
    }
    async fn delete(&self, key: &str) -> Result<(), muniment::StoreError> {
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[muniment::WriteOp]) -> Result<(), muniment::StoreError> {
        self.inner.apply(ops).await
    }
}

#[test]
fn owner_lifecycle_reentered_actual_a_waits_for_retired_a_write_before_loading_or_new_save() {
    use std::future::Future;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    pollster::block_on(async {
        let (backend, mut app, mut old, member) = fixture();
        let a_id = app.host.graph_session().id();
        app.host.persist(1).await.unwrap();
        persist(&mut old, backend.clone()).await;
        let proposal = old.prepare_save(SaveKind::Visibility, 21).unwrap();
        let delay = DelayedPut {
            inner: backend.clone(),
            started: Arc::new(AtomicBool::new(false)),
            released: Arc::new(AtomicBool::new(false)),
        };
        let mut pending = Box::pin(proposal.save(delay.clone()));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(pending.as_mut().poll(&mut cx).is_pending());
        assert!(
            delay.started.load(Ordering::SeqCst),
            "old A put is actually in flight"
        );
        let mut fence = FormeWriteFence::default();
        let write = fence.retire(a_id.0);
        let bytes = codicil(&app, member);
        app.host.replace_with_product_codicil(&bytes).unwrap();
        let b = app.host.graph_session().id().0;
        assert!(!fence.blocks(b), "unrelated owner not blocked");
        let b_loaded = load(&backend, b, app.host.graph()).await.unwrap();
        assert_eq!(b_loaded.committed().document.graph_id, b);
        app.host.persist(2).await.unwrap();
        let host_a = crate::mere_host::MereHost::open_session(
            backend.clone(),
            a_id,
            SelectedPersonaRef {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                profile: "profile:forme-lifecycle".into(),
            },
            crate::mere_host::fixture_handlers(),
            crate::access::AccessContext {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                device: crate::mere_host::FIXTURE_DEVICE_TWO_ADDRESS.into(),
                at_ms: 30,
            },
        )
        .await
        .unwrap();
        let app_a = App::new(host_a);
        assert_eq!(app_a.host.graph_session().id().0, a_id.0);
        assert!(
            fence.blocks(a_id.0),
            "new A cannot load/save ahead of old A writer"
        );
        delay.released.store(true, Ordering::SeqCst);
        old.finish_save(proposal.clone(), pending.await).unwrap();
        fence.finish(write);
        let mut current = load(&backend, a_id.0, app_a.host.graph()).await.unwrap();
        assert_eq!(
            current.view().visible,
            old.committed().visible,
            "reload observes settled old write"
        );
        current.unlock().unwrap();
        current
            .edit(None, 32, |w| {
                w.bounds[0] -= 13.;
                Ok(())
            })
            .unwrap();
        let new_write = current.prepare_save(SaveKind::Apply, 33).unwrap();
        let result = new_write.save(backend.clone()).await;
        current.finish_save(new_write, result).unwrap();
        let reopened = load(&backend, a_id.0, app_a.host.graph()).await.unwrap();
        assert_eq!(reopened.committed().bounds, current.committed().bounds);
    });
}

#[test]
fn owner_lifecycle_missing_access_tile_and_drop_target_are_noninteractive_without_identity_loss() {
    let (_, mut app, mut session, _) = fixture();
    let added = crate::local_intake::create_address(
        &mut app,
        "https://example.test/missing-tile",
        "Missing",
    )
    .unwrap();
    session.unlock().unwrap();
    session
        .edit(None, 1, |w| w.open(added, app.host.graph()))
        .unwrap();
    let tile = session.view().tile_for_member(added).unwrap();
    let before = serde_json::to_vec(session.view()).unwrap();
    app.host.undo_now(GRAPHSHELL).unwrap();
    assert!(
        session
            .edit(None, 2, |w| w.event(
                workbench::TileEvent::Activated(tile),
                app.host.graph()
            ))
            .is_err()
    );
    assert!(
        session
            .edit(None, 3, |w| w
                .event(workbench::TileEvent::Closed(tile), app.host.graph()))
            .is_err()
    );
    assert!(
        session
            .drop_preview(added, (0., 0.), app.host.graph())
            .is_err()
    );
    fn has_tile(tree: &workbench::TileTree, tile: workbench::TileId) -> bool {
        match tree {
            workbench::TileTree::Stack(stack) => stack.tabs.iter().any(|t| t.id == tile),
            workbench::TileTree::Split { children, .. } => {
                children.iter().any(|c| has_tile(&c.tree, tile))
            },
        }
    }
    assert!(!has_tile(
        &session.view().tile_tree(app.host.graph()).unwrap(),
        tile
    ));
    assert_eq!(serde_json::to_vec(session.view()).unwrap(), before);
    app.host.redo_now(GRAPHSHELL).unwrap();
    assert!(has_tile(
        &session.view().tile_tree(app.host.graph()).unwrap(),
        tile
    ));
    assert_eq!(session.view().tile_for_member(added), Some(tile));
}

#[test]
fn owner_lifecycle_real_scene_restore_reprojects_same_owner_and_failed_restore_preserves_region() {
    let (_, app, session, _) = fixture();
    let owner = app.host.graph_session().id().0;
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas
        .set_forme_region(
            crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                .unwrap(),
        )
        .unwrap();
    let before = canvas.forme_region().unwrap().clone();
    let scene = canvas
        .capture_scene(
            "Graphshell working scene",
            mere::canvas::physics_controls::FREE_ARRANGEMENT,
            "",
        )
        .unwrap();
    canvas
        .restore_scene(app.host.graph(), &scene, (900, 600))
        .unwrap();
    assert!(
        canvas.forme_region().is_none(),
        "real accepted scene restoration resets physical region"
    );
    canvas
        .set_forme_region(
            crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(canvas.forme_region(), Some(&before));
    let mut invalid = scene;
    invalid.camera_zoom = f32::NAN;
    assert!(
        canvas
            .restore_scene(app.host.graph(), &invalid, (900, 600))
            .is_err()
    );
    assert_eq!(
        canvas.forme_region(),
        Some(&before),
        "rejected pre-mutation scene keeps region"
    );
}

#[test]
fn owner_lifecycle_edge_drop_refuses_missing_target_without_mutating_draft() {
    let (_, mut app, mut session, member) = fixture();
    let added = crate::local_intake::create_address(
        &mut app,
        "https://example.test/missing-edge-target",
        "Missing target",
    )
    .unwrap();
    session.unlock().unwrap();
    session
        .edit(None, 1, |w| w.open(added, app.host.graph()))
        .unwrap();
    let source = session.view().tile_for_member(member).unwrap();
    let target = session.view().tile_for_member(added).unwrap();
    app.host.undo_now(GRAPHSHELL).unwrap();
    assert!(app.host.graph().get_node_by_id(member).is_some());
    assert!(app.host.graph().get_node_by_id(added).is_none());
    let before = serde_json::to_vec(session.view()).unwrap();
    assert!(
        session
            .edit(None, 2, |w| w.event(
                workbench::TileEvent::Dragged {
                    tile: source,
                    to: workbench::DropTarget::Edge {
                        tile: target,
                        edge: workbench::Edge::Right
                    },
                },
                app.host.graph()
            ))
            .is_err(),
        "a present source cannot target a missing saved access"
    );
    assert_eq!(serde_json::to_vec(session.view()).unwrap(), before);
    app.host.redo_now(GRAPHSHELL).unwrap();
    session
        .edit(None, 3, |w| {
            w.event(
                workbench::TileEvent::Dragged {
                    tile: source,
                    to: workbench::DropTarget::Edge {
                        tile: target,
                        edge: workbench::Edge::Right,
                    },
                },
                app.host.graph(),
            )
        })
        .unwrap();
    assert_ne!(serde_json::to_vec(session.view()).unwrap(), before);
}

#[test]
fn owner_lifecycle_actual_button_preview_projects_missing_cell_and_checks_owner() {
    let (_, mut app, mut session, moving) = fixture();
    let owner = app.host.graph_session().id().0;
    let target = app
        .host
        .graph()
        .get_node_by_url(crate::mere_host::FIXTURE_GRANT_ADDRESS)
        .unwrap()
        .1
        .id;
    let absent = crate::local_intake::create_address(
        &mut app,
        "https://example.test/preview-absent",
        "Absent",
    )
    .unwrap();
    session.unlock().unwrap();
    session
        .edit(None, 1, |w| {
            w.open(target, app.host.graph())?;
            w.open(absent, app.host.graph())?;
            w.event(
                workbench::TileEvent::Dragged {
                    tile: w.tile_for_member(absent).unwrap(),
                    to: workbench::DropTarget::Edge {
                        tile: w.tile_for_member(target).unwrap(),
                        edge: workbench::Edge::Right,
                    },
                },
                app.host.graph(),
            )
        })
        .unwrap();
    app.host.undo_now(GRAPHSHELL).unwrap();
    let before = serde_json::to_vec(session.view()).unwrap();
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    let candidate = session
        .edge_preview_candidate(
            owner,
            moving,
            target,
            workbench::Edge::Bottom,
            app.host.graph(),
        )
        .unwrap();
    assert!(
        candidate.region().cells.iter().any(|c| c.member == absent),
        "raw candidate still preserves the missing cell identity"
    );
    let projected =
        crate::forme_workspace::project_region(&candidate, owner, app.host.graph()).unwrap();
    canvas.set_forme_region(projected).unwrap();
    let region = canvas.forme_region().unwrap().clone();
    assert!(region.cells.iter().any(|c| c.member == moving));
    assert!(region.cells.iter().any(|c| c.member == target));
    assert!(!region.cells.iter().any(|c| c.member == absent));
    assert_eq!(
        serde_json::to_vec(session.view()).unwrap(),
        before,
        "preview does not commit or rewrite history"
    );
    assert!(
        session
            .edge_preview_candidate(
                Uuid::new_v4(),
                moving,
                target,
                workbench::Edge::Left,
                app.host.graph(),
            )
            .is_err()
    );
    assert!(
        session
            .edge_preview_candidate(
                owner,
                moving,
                absent,
                workbench::Edge::Left,
                app.host.graph(),
            )
            .is_err()
    );
    assert_eq!(canvas.forme_region(), Some(&region));
}

#[test]
fn owner_lifecycle_actual_open_rejects_invalid_camera_before_owner_or_canvas_changes() {
    pollster::block_on(async {
        let (backend, mut app, mut session, member) = fixture();
        let owner = app.host.graph_session().id().0;
        persist(&mut session, backend.clone()).await;
        let old_slot = backend.get(&slot(owner)).await.unwrap().unwrap();
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        canvas
            .set_forme_region(
                crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                    .unwrap(),
            )
            .unwrap();
        let region = canvas.forme_region().unwrap().clone();
        let graph = serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap();
        let mut scene = canvas
            .capture_scene(
                "Graphshell working scene",
                mere::canvas::physics_controls::FREE_ARRANGEMENT,
                "",
            )
            .unwrap();
        scene.camera_zoom = -1.0;
        let mut record: serde_json::Value = serde_json::from_slice(&codicil(&app, member)).unwrap();
        record["scene"] = serde_json::to_value(scene).unwrap();
        let bytes = serde_json::to_vec(&record).unwrap();
        assert!(
            crate::local_scene::open_codicil(&mut app.host, &mut canvas, &bytes, (900, 600))
                .is_err()
        );
        assert_eq!(app.host.graph_session().id().0, owner);
        assert_eq!(
            serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap(),
            graph
        );
        assert_eq!(canvas.forme_region(), Some(&region));
        assert_eq!(backend.get(&slot(owner)).await.unwrap().unwrap(), old_slot);
    });
}

#[test]
fn owner_lifecycle_actual_scene_open_installs_new_graph_and_empty_owner_model() {
    pollster::block_on(async {
        let (backend, mut app, mut session, member) = fixture();
        let old_owner = app.host.graph_session().id().0;
        persist(&mut session, backend.clone()).await;
        let old_slot = backend.get(&slot(old_owner)).await.unwrap().unwrap();
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        canvas
            .set_forme_region(
                crate::forme_workspace::project_region(session.view(), old_owner, app.host.graph())
                    .unwrap(),
            )
            .unwrap();
        let bytes = codicil(&app, member);
        let accepted =
            crate::local_scene::open_codicil(&mut app.host, &mut canvas, &bytes, (900, 600))
                .unwrap();
        assert!(accepted.view_error.is_none());
        assert_eq!(accepted.owner, app.host.graph_session().id().0);
        assert_ne!(accepted.owner, old_owner);
        assert_eq!(accepted.receipt.nodes, 1);
        assert_eq!(canvas.graph().nodes().count(), 1);
        assert_eq!(
            serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap(),
            serde_json::to_value(app.host.graph().to_snapshot_at(0)).unwrap()
        );
        assert!(canvas.forme_region().is_none());
        let loaded = load(&backend, accepted.owner, app.host.graph())
            .await
            .unwrap();
        assert!(loaded.view().members().is_empty());
        assert!(
            crate::forme_workspace::project_region(loaded.view(), accepted.owner, canvas.graph())
                .unwrap()
                .is_none()
        );
        assert_eq!(
            backend.get(&slot(old_owner)).await.unwrap().unwrap(),
            old_slot
        );
        assert!(backend.get(&slot(accepted.owner)).await.unwrap().is_none());
    });
}

#[test]
fn owner_lifecycle_postaccepted_view_error_cannot_leave_previous_canvas_graph() {
    let (_, mut app, session, member) = fixture();
    let old_owner = app.host.graph_session().id().0;
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas
        .set_forme_region(
            crate::forme_workspace::project_region(session.view(), old_owner, app.host.graph())
                .unwrap(),
        )
        .unwrap();
    let mut invalid = canvas
        .capture_scene(
            "Graphshell working scene",
            mere::canvas::physics_controls::FREE_ARRANGEMENT,
            "",
        )
        .unwrap();
    invalid.camera_zoom = -1.0;
    let bytes = codicil(&app, member);
    app.host.replace_with_product_codicil(&bytes).unwrap();
    assert_ne!(app.host.graph_session().id().0, old_owner);
    assert!(canvas.graph().nodes().count() > app.host.graph().nodes().count());
    assert!(
        crate::local_scene::install_accepted_codicil(
            &mut canvas,
            app.host.graph(),
            Some(&invalid),
            (900, 600)
        )
        .is_some()
    );
    assert_eq!(
        serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap(),
        serde_json::to_value(app.host.graph().to_snapshot_at(0)).unwrap()
    );
    assert!(canvas.forme_region().is_none());
}

#[test]
fn owner_lifecycle_actual_history_forget_completion_updates_canvas_before_projection() {
    pollster::block_on(async {
        use crate::capture::{
            BrowserHistoryCapture, BrowserVisit, ForgetMode, HistoryCapturePolicy,
        };
        let (backend, mut app, mut session, member) = fixture();
        let owner = app.host.graph_session().id().0;
        let mut store = backend;
        let mut capture =
            BrowserHistoryCapture::load(&mut store, HistoryCapturePolicy::consented())
                .await
                .unwrap();
        let address = "https://example.test/forme-history-forget";
        capture
            .ingest_batch(
                &mut app.host,
                &mut store,
                [BrowserVisit {
                    source: "firefox".into(),
                    visit_id: Some("forme-forget-1".into()),
                    source_scope: None,
                    parent: Default::default(),
                    original_unix_micros: None,
                    source_is_local: None,
                    url: address.into(),
                    title: Some("Captured".into()),
                    favicon_url: None,
                    referrer_url: None,
                    transition: "typed".into(),
                    at_ms: 1000,
                    private: false,
                }],
                FIXTURE_PERSONA_ADDRESS,
                "device:forme-lifecycle",
                1,
            )
            .await
            .unwrap();
        let captured = app.host.graph().get_node_by_url(address).unwrap().1.id;
        session.unlock().unwrap();
        session
            .edit(None, 1, |w| {
                w.open(captured, app.host.graph())?;
                w.event(
                    workbench::TileEvent::Dragged {
                        tile: w.tile_for_member(captured).unwrap(),
                        to: workbench::DropTarget::Edge {
                            tile: w.tile_for_member(member).unwrap(),
                            edge: workbench::Edge::Right,
                        },
                    },
                    app.host.graph(),
                )
            })
            .unwrap();
        let before = serde_json::to_vec(session.view()).unwrap();
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        canvas.set_camera(mere::canvas::CameraView {
            offset: (71.0, -22.0),
            zoom: 1.7,
        });
        canvas.set_physics_paused(true);
        canvas
            .set_forme_region(
                crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                    .unwrap(),
            )
            .unwrap();
        capture
            .forget_url(
                &mut app.host,
                &mut store,
                address,
                ForgetMode::RemoveCapturedObject,
                2,
            )
            .await
            .unwrap();
        assert!(app.host.graph().get_node_by_id(captured).is_none());
        assert!(
            canvas.graph().get_node_by_id(captured).is_some(),
            "the real async request returns before completion installs its owner"
        );
        crate::local_scene::reconcile_completed_graph(
            &mut canvas,
            Some(owner),
            app.host.graph_session().id().0,
            app.host.graph(),
        );
        assert!(canvas.graph().get_node_by_id(captured).is_none());
        canvas
            .set_forme_region(
                crate::forme_workspace::project_region(session.view(), owner, canvas.graph())
                    .unwrap(),
            )
            .unwrap();
        let region = canvas.forme_region().unwrap();
        assert!(region.cells.iter().any(|c| c.member == member));
        assert!(!region.cells.iter().any(|c| c.member == captured));
        assert_eq!(canvas.camera().offset, (71.0, -22.0));
        assert_eq!(canvas.camera().zoom, 1.7);
        assert!(canvas.physics_paused());
        assert_eq!(serde_json::to_vec(session.view()).unwrap(), before);
    });
}

#[test]
fn owner_lifecycle_async_completion_new_actual_owner_releases_old_region() {
    let (_, mut app, session, member) = fixture();
    let a = app.host.graph_session().id().0;
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas
        .set_forme_region(
            crate::forme_workspace::project_region(session.view(), a, app.host.graph()).unwrap(),
        )
        .unwrap();
    let bytes = codicil(&app, member);
    app.host.replace_with_product_codicil(&bytes).unwrap();
    let b = app.host.graph_session().id().0;
    crate::local_scene::reconcile_completed_graph(&mut canvas, Some(a), b, app.host.graph());
    assert_ne!(a, b);
    assert!(canvas.forme_region().is_none());
    assert_eq!(
        canvas.graph().nodes().count(),
        app.host.graph().nodes().count()
    );
}

#[derive(Clone)]
struct RejectWrites {
    inner: MemoryBackend,
    reject: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Backend for RejectWrites {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, muniment::StoreError> {
        self.inner.get(key).await
    }
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), muniment::StoreError> {
        if self.reject.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(muniment::StoreError::Backend(
                "lifecycle write refused".into(),
            ));
        }
        self.inner.put(key, bytes).await
    }
    async fn delete(&self, key: &str) -> Result<(), muniment::StoreError> {
        self.inner.delete(key).await
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, muniment::StoreError> {
        self.inner.scan(start, end).await
    }
    async fn apply(&self, ops: &[muniment::WriteOp]) -> Result<(), muniment::StoreError> {
        if self.reject.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(muniment::StoreError::Backend(
                "lifecycle transaction refused".into(),
            ));
        }
        self.inner.apply(ops).await
    }
}

#[test]
fn owner_lifecycle_failed_real_intake_persistence_still_reconciles_returned_app() {
    pollster::block_on(async {
        let backend = RejectWrites {
            inner: MemoryBackend::new(),
            reject: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let mut app = GraphshellApp::fixture(
            backend.clone(),
            SelectedPersonaRef {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                profile: "profile:forme-intake-failure".into(),
            },
        )
        .unwrap();
        let owner = app.host.graph_session().id().0;
        let member = app
            .host
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        let mut workspace = FormeWorkspace::new(owner);
        workspace.open(member, app.host.graph()).unwrap();
        let session = FormeSession::new(workspace);
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        canvas
            .set_forme_region(
                crate::forme_workspace::project_region(session.view(), owner, app.host.graph())
                    .unwrap(),
            )
            .unwrap();
        let before = canvas.forme_region().unwrap().clone();
        canvas.set_camera(mere::canvas::CameraView {
            offset: (11.0, 26.0),
            zoom: 1.3,
        });
        canvas.set_physics_paused(true);
        let added = crate::local_intake::create_address(
            &mut app,
            "https://example.test/failed-forme-intake",
            "Unsaved intake",
        )
        .unwrap();
        backend
            .reject
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            crate::local_intake::persist_intake(&mut app, added, 3)
                .await
                .is_err()
        );
        assert!(
            app.host.graph().get_node_by_id(added).is_some(),
            "failed persistence retains the real admitted in-memory edit"
        );
        assert!(canvas.graph().get_node_by_id(added).is_none());
        crate::local_scene::reconcile_completed_graph(
            &mut canvas,
            Some(owner),
            app.host.graph_session().id().0,
            app.host.graph(),
        );
        assert!(canvas.graph().get_node_by_id(added).is_some());
        canvas
            .set_forme_region(
                crate::forme_workspace::project_region(session.view(), owner, canvas.graph())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(canvas.forme_region(), Some(&before));
        assert_eq!(canvas.camera().offset, (11.0, 26.0));
        assert_eq!(canvas.camera().zoom, 1.3);
        assert!(canvas.physics_paused());
    });
}

#[test]
fn owner_lifecycle_failed_real_detail_persistence_reconciles_facet_and_preserves_view() {
    pollster::block_on(async {
        let backend = RejectWrites {
            inner: MemoryBackend::new(),
            reject: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let mut app = GraphshellApp::fixture(
            backend.clone(),
            SelectedPersonaRef {
                persona: FIXTURE_PERSONA_ADDRESS.into(),
                profile: "profile:forme-detail-failure".into(),
            },
        )
        .unwrap();
        let owner = app.host.graph_session().id().0;
        let member = app
            .host
            .graph()
            .get_node_by_url(FIXTURE_WEB_ADDRESS)
            .unwrap()
            .1
            .id;
        let mut canvas = Canvas::with_graph(app.host.graph().clone());
        canvas.set_camera(mere::canvas::CameraView {
            offset: (39.0, -18.0),
            zoom: 1.4,
        });
        canvas.set_physics_paused(true);
        backend
            .reject
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            crate::local_edit::save_detail_action(
                &mut app,
                member,
                crate::local_edit::DetailAction::Facet {
                    id: "example.review".into(),
                    json: "{\"reviewed\":true}".into()
                },
                4
            )
            .await
            .is_err()
        );
        let facet = chartulary::FacetId::new("example.review");
        assert_eq!(
            app.host.graph().facets().get(&member, &facet),
            Some(&serde_json::json!({"reviewed":true}))
        );
        assert!(canvas.graph().facets().get(&member, &facet).is_none());
        crate::local_scene::reconcile_completed_graph(
            &mut canvas,
            Some(owner),
            app.host.graph_session().id().0,
            app.host.graph(),
        );
        assert_eq!(
            canvas.graph().facets().get(&member, &facet),
            app.host.graph().facets().get(&member, &facet)
        );
        assert_eq!(canvas.camera().offset, (39.0, -18.0));
        assert_eq!(canvas.camera().zoom, 1.4);
        assert!(canvas.physics_paused());
    });
}

fn unpaused_parked_fixture() -> (RejectWrites, GraphshellApp<RejectWrites>, Canvas, Uuid) {
    let backend = RejectWrites {
        inner: MemoryBackend::new(),
        reject: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    let app = GraphshellApp::fixture(
        backend.clone(),
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.into(),
            profile: "profile:forme-unpaused".into(),
        },
    )
    .unwrap();
    let member = app
        .host
        .graph()
        .get_node_by_url(FIXTURE_WEB_ADDRESS)
        .unwrap()
        .1
        .id;
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas.set_camera(mere::canvas::CameraView {
        offset: (24.0, -16.0),
        zoom: 1.6,
    });
    assert!(
        !canvas.physics_paused(),
        "this control exercises live, unpaused physics"
    );
    canvas.park_physics();
    assert!(!canvas.is_settling());
    (backend, app, canvas, member)
}
fn positions(canvas: &Canvas) -> Vec<(Uuid, Option<mere::kernel::geometry::PortablePoint>)> {
    canvas
        .graph()
        .nodes()
        .map(|(key, node)| (node.id, canvas.world_position_of(key)))
        .collect()
}

#[test]
fn owner_lifecycle_successful_actual_metadata_completion_keeps_unpaused_physics_parked() {
    pollster::block_on(async {
        let (_, mut app, mut canvas, member) = unpaused_parked_fixture();
        let owner = app.host.graph_session().id().0;
        let demand = canvas.physics_tick_demand();
        let before = positions(&canvas);
        let saved =
            crate::local_edit::save_metadata(&mut app, member, "Retained title", "reviewed", 5)
                .await
                .unwrap();
        crate::local_scene::reconcile_completed_graph(
            &mut canvas,
            Some(owner),
            app.host.graph_session().id().0,
            app.host.graph(),
        );
        assert!(!canvas.physics_paused());
        assert!(
            !canvas.is_settling(),
            "metadata completion must not restart a parked live graph"
        );
        assert_eq!(canvas.physics_tick_demand(), demand);
        assert_eq!(positions(&canvas), before);
        assert_eq!(canvas.camera().offset, (24.0, -16.0));
        assert_eq!(canvas.camera().zoom, 1.6);
        assert_eq!(
            canvas.graph().get_node_by_id(member).unwrap().1.title,
            saved.title
        );
        let key = canvas.graph().get_node_key_by_id(member).unwrap();
        assert_eq!(
            canvas.graph().node_content_tags(key),
            app.host.graph().node_content_tags(key)
        );
    });
}

#[test]
fn owner_lifecycle_failed_actual_facet_completion_keeps_unpaused_physics_parked() {
    pollster::block_on(async {
        let (backend, mut app, mut canvas, member) = unpaused_parked_fixture();
        let owner = app.host.graph_session().id().0;
        let demand = canvas.physics_tick_demand();
        let before = positions(&canvas);
        backend
            .reject
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(
            crate::local_edit::save_detail_action(
                &mut app,
                member,
                crate::local_edit::DetailAction::Facet {
                    id: "example.parked".into(),
                    json: "true".into()
                },
                6
            )
            .await
            .is_err()
        );
        crate::local_scene::reconcile_completed_graph(
            &mut canvas,
            Some(owner),
            app.host.graph_session().id().0,
            app.host.graph(),
        );
        assert!(!canvas.physics_paused());
        assert!(
            !canvas.is_settling(),
            "failed persistence retains facet data without restarting live physics"
        );
        assert_eq!(canvas.physics_tick_demand(), demand);
        assert_eq!(positions(&canvas), before);
        assert_eq!(
            canvas
                .graph()
                .facets()
                .get(&member, &chartulary::FacetId::new("example.parked")),
            Some(&serde_json::json!(true))
        );
    });
}

#[test]
fn owner_lifecycle_actual_structural_completion_still_ingests_addition_and_removal() {
    let (_, mut app, mut canvas, _) = unpaused_parked_fixture();
    let owner = app.host.graph_session().id().0;
    let added = crate::local_intake::create_address(
        &mut app,
        "https://example.test/structural-unpaused",
        "New physical body",
    )
    .unwrap();
    assert!(
        canvas
            .refresh_same_topology_graph(app.host.graph())
            .is_err()
    );
    assert!(
        canvas.graph().get_node_by_id(added).is_none(),
        "refused metadata refresh is atomic"
    );
    crate::local_scene::reconcile_completed_graph(
        &mut canvas,
        Some(owner),
        app.host.graph_session().id().0,
        app.host.graph(),
    );
    let key = canvas.graph().get_node_key_by_id(added).unwrap();
    assert!(
        canvas.world_position_of(key).is_some(),
        "structural ingest seeds a real new body"
    );
    assert!(
        canvas.is_settling(),
        "structural changes still wake the live graph"
    );
    canvas.park_physics();
    assert!(app.host.undo_now(GRAPHSHELL).unwrap().is_some());
    assert!(app.host.graph().get_node_by_id(added).is_none());
    assert!(
        canvas
            .refresh_same_topology_graph(app.host.graph())
            .is_err()
    );
    assert!(canvas.graph().get_node_by_id(added).is_some());
    crate::local_scene::reconcile_completed_graph(
        &mut canvas,
        Some(owner),
        app.host.graph_session().id().0,
        app.host.graph(),
    );
    assert!(canvas.graph().get_node_by_id(added).is_none());
    assert!(canvas.is_settling());
    assert!(!canvas.physics_paused());
}

#[test]
fn owner_lifecycle_same_topology_refresh_refuses_relation_changes_and_key_remaps() {
    let (_, mut app, mut canvas, member) = unpaused_parked_fixture();
    let owner = app.host.graph_session().id().0;
    let before = serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap();
    let demand = canvas.physics_tick_demand();
    let target = app
        .host
        .graph()
        .get_node_by_url(crate::mere_host::FIXTURE_GRANT_ADDRESS)
        .unwrap()
        .1
        .id;
    app.host
        .assert_product_relation(member, target, crate::product::EditableRelation::Cites)
        .unwrap();
    assert!(
        canvas
            .refresh_same_topology_graph(app.host.graph())
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap(),
        before
    );
    assert_eq!(canvas.physics_tick_demand(), demand);
    assert!(!canvas.is_settling());
    let mut snapshot = canvas.graph().to_snapshot_at(0);
    assert!(snapshot.nodes.len() > 1);
    let selected_index = snapshot
        .nodes
        .iter()
        .position(|node| node.node_id == member.to_string())
        .unwrap();
    let other_index = if selected_index == 0 { 1 } else { 0 };
    snapshot.nodes.swap(selected_index, other_index);
    let remapped = mere::kernel::graph::Graph::try_from_recorded_snapshot(&snapshot).unwrap();
    assert_ne!(
        canvas.graph().get_node_key_by_id(member),
        remapped.get_node_key_by_id(member)
    );
    assert!(canvas.refresh_same_topology_graph(&remapped).is_err());
    assert_eq!(
        serde_json::to_value(canvas.graph().to_snapshot_at(0)).unwrap(),
        before
    );
    assert_eq!(canvas.physics_tick_demand(), demand);
    crate::local_scene::reconcile_completed_graph(&mut canvas, Some(owner), owner, &remapped);
    assert_eq!(
        canvas.graph().get_node_key_by_id(member),
        remapped.get_node_key_by_id(member)
    );
    assert!(canvas.forme_region().is_none());
}
