// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Real local file -> ordinary route -> shared host preparation/open boundary.
//! No native window/GPU or network submission is performed by this fixture.

use super::*;
use crate::app::App;
use crate::ui::{MicronFormStage, OmnibarMode};
use std::sync::Arc;
use uuid::Uuid;

const SOURCE: &str = "#!c=0\n>Original\n`[Submit`7fc8950ec4e50695be76eddc8e539ee8:/page/capture.mu`mnprobe_text|mnprobe_mask|mnprobe_fixed=ready]\nText: `<mnprobe_text`seed-text>\nMasked: `<!|mnprobe_mask`mask-seed>\n";

struct LocalPage {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
    url: String,
}

impl LocalPage {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source.mu");
        std::fs::write(&path, SOURCE).unwrap();
        let url = url::Url::from_file_path(&path).unwrap().to_string();
        Self { _directory: directory, path, url }
    }
}

fn spawn_from(effects: Vec<Effect>) -> (Uuid, String) {
    let mut spawns = effects.into_iter().filter_map(|effect| match effect {
        Effect::SpawnContent { node, url } => Some((node, url)),
        _ => None,
    });
    let spawn = spawns.next().expect("App emitted actual SpawnContent");
    assert!(spawns.next().is_none(), "one admitted local spawn");
    spawn
}

fn request(app: &mut App, page: &LocalPage) -> (Uuid, String) {
    app.update(Action::OpenAddress(page.url.clone()));
    let spawn = spawn_from(app.update(Action::ToggleNodeContent));
    assert_eq!(spawn.1, page.url);
    spawn
}

fn prepare(app: &App, spawn: &(Uuid, String)) -> Result<Option<PreparedContentController>, String> {
    let fetched = app.content.fetched(spawn.0, &spawn.1).cloned();
    // Same product pin + standard policy used by Shell's SpawnContent arm.
    let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone"),
        view: None,
        node: None,
        address: spawn.1.clone(),
        content_type: fetched.as_ref().and_then(|source| source.content_type.clone()),
        pinned_engine: crate::nomadnet::is_micron_address(&spawn.1)
            .then(|| crate::nomadnet::ENGINE_ID.to_owned()),
    });
    assert_eq!(decision.engine_id, crate::nomadnet::ENGINE_ID);
    assert_eq!(dispatch_content_spawn(&decision.engine_id, false, &mut Vec::new(), spawn.0, &spawn.1), ContentSpawnDispatch::Session);
    prepare_content_controller(app, spawn.0, &spawn.1, &decision.engine_id, fetched, None, (480, 320))
}

fn open(app: &App, prepared: PreparedContentController) -> Result<Option<OpenedContentController>, String> {
    open_prepared_content_controller(
        app, prepared,
        Arc::new(super::super::standard_content_engines()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        super::super::ShellClock(std::time::Instant::now()),
    )
}

fn accept(app: &mut App, spawn: &(Uuid, String), opened: &mut OpenedContentController) {
    if let Some(source) = opened.source.take() {
        app.apply_update(source);
    }
    // State acknowledgement follows a real successful controller; fact projection
    // and Shell entry installation are outside this headless host-boundary fixture.
    app.apply_update(Update::ContentSpawned { node: spawn.0, facts: None });
}

fn assert_stage(app: &App, expected: MicronFormStage) {
    match &app.omnibar.mode {
        OmnibarMode::MicronForm(prompt) => assert_eq!(prompt.stage, expected),
        mode => panic!("expected editor {expected:?}, got {mode:?}"),
    }
}

#[test]
fn local_micron_spawn_opens_field_mask_and_confirm_from_real_file() {
    let page = LocalPage::new();
    let mut app = App::test_stub_at(page._directory.path().join("app"));
    let spawn = request(&mut app, &page);
    assert!(app.content.fetched(spawn.0, &spawn.1).is_none());
    let prepared = prepare(&app, &spawn).unwrap().unwrap();
    let mut opened = open(&app, prepared).unwrap().unwrap();
    assert_eq!(opened.controller.session().inspect().unwrap().title.as_deref(), Some("Original"));
    let scene = opened.controller.session_mut().frame(480, 320);
    assert!(scene.ops.iter().any(|op| matches!(op, netrender::SceneOp::GlyphRun(_))));
    accept(&mut app, &spawn, &mut opened);
    let effects = app.update(Action::ComposeFocusedMicronForm);
    assert!(!effects.iter().any(|e| matches!(e, Effect::SubmitMicron { .. })));
    assert_stage(&app, MicronFormStage::Field(0));
    assert_eq!(app.content.fetched(spawn.0, &spawn.1).unwrap().body, SOURCE);
    app.update(Action::OmnibarCommit);
    assert_stage(&app, MicronFormStage::Field(1));
    for _ in 0.."mask-seed".len() { app.update(Action::OmnibarBackspace); }
    app.update(Action::OmnibarInsert("not-for-debug".into()));
    assert!(app.omnibar.sensitive());
    assert!(!format!("{:?}", app.omnibar).contains("not-for-debug"));
    app.update(Action::OmnibarCommit);
    assert_stage(&app, MicronFormStage::Confirm);
    app.update(Action::OmnibarInsert("send".into()));
    let effects = app.update(Action::OmnibarCommit);
    let requests: Vec<_> = effects.iter().filter_map(|effect| match effect {
        Effect::SubmitMicron { source, submission, .. } => Some((source, submission)),
        _ => None,
    }).collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(*requests[0].0, Some(spawn.0));
    assert_eq!(requests[0].1.target, "7fc8950ec4e50695be76eddc8e539ee8:/page/capture.mu");
    assert_eq!(requests[0].1.values.get("field_mnprobe_text").map(String::as_str), Some("seed-text"));
    assert_eq!(requests[0].1.values.get("field_mnprobe_mask").map(String::as_str), Some("not-for-debug"));
    assert_eq!(requests[0].1.values.get("var_mnprobe_fixed").map(String::as_str), Some("ready"));
    assert!(!format!("{effects:?}").contains("not-for-debug"));
}

#[test]
fn local_micron_spawn_uses_one_held_body_after_file_changes() {
    for remove in [false, true] {
        let page = LocalPage::new();
        let mut app = App::test_stub_at(page._directory.path().join("app"));
        let spawn = request(&mut app, &page);
        let prepared = prepare(&app, &spawn).unwrap().unwrap();
        assert!(app.content.fetched(spawn.0, &spawn.1).is_none(), "preparation does not publish");
        if remove { std::fs::remove_file(&page.path).unwrap(); }
        else { std::fs::write(&page.path, ">Replacement\nChanged on disk\n").unwrap(); }
        let mut opened = open(&app, prepared).unwrap().unwrap();
        assert_eq!(opened.controller.session().inspect().unwrap().title.as_deref(), Some("Original"));
        accept(&mut app, &spawn, &mut opened);
        let held = app.content.fetched(spawn.0, &spawn.1).unwrap();
        assert_eq!(held.bytes, SOURCE.as_bytes());
        assert_eq!(held.body, SOURCE);
        app.update(Action::ComposeFocusedMicronForm);
        assert_stage(&app, MicronFormStage::Field(0));
    }
}

#[test]
fn local_micron_spawn_reload_missing_file_keeps_source_absent() {
    let page = LocalPage::new();
    let mut app = App::test_stub_at(page._directory.path().join("app"));
    let spawn = request(&mut app, &page);
    let mut opened = open(&app, prepare(&app, &spawn).unwrap().unwrap()).unwrap().unwrap();
    accept(&mut app, &spawn, &mut opened);
    std::fs::remove_file(&page.path).unwrap();
    let effects = app.update(Action::Reload);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::CloseContent { node } if *node == spawn.0)));
    let reload = spawn_from(effects);
    assert_eq!(reload, spawn);
    assert!(app.content.fetched(spawn.0, &spawn.1).is_none());
    // Both baseline engine fallback and repaired host preparation must fail.
    let failed = prepare(&app, &reload).and_then(|prepared| open(&app, prepared.expect("current request")));
    let error = match failed { Err(error) => error, Ok(_) => panic!("missing file must fail") };
    app.apply_update(Update::ContentFailed { node: spawn.0, error });
    assert!(matches!(app.content.get(spawn.0), Some(crate::content::NodeContent::Failed(_))));
    assert!(app.content.fetched(spawn.0, &spawn.1).is_none());
    let effects = app.update(Action::ComposeFocusedMicronForm);
    assert!(!matches!(app.omnibar.mode, OmnibarMode::MicronForm(_)));
    assert!(!effects.iter().any(|e| matches!(e, Effect::SubmitMicron { .. })));
}

#[test]
fn local_micron_spawn_rejects_stale_uuid_and_address_before_open() {
    for remove_node in [false, true] {
        let page = LocalPage::new();
        let mut app = App::test_stub_at(page._directory.path().join("app"));
        let spawn = request(&mut app, &page);
        let prepared = prepare(&app, &spawn).unwrap().unwrap();
        if remove_node {
            app.update(Action::DeleteFocusedNode);
        } else {
            let graph = app.graph_runtimes.graph_containing_member(spawn.0).unwrap();
            assert!(app.graph_runtimes.canvas_mut(graph).unwrap().navigate_member(spawn.0, "file:///changed.mu"));
        }
        std::fs::remove_file(&page.path).unwrap();
        assert!(prepare(&app, &spawn).unwrap().is_none(), "stale rejection precedes file read");
        assert!(open(&app, prepared).unwrap().is_none(), "stale prepared source cannot open/publish");
        assert!(app.content.fetched(spawn.0, &spawn.1).is_none());
    }
}

#[test]
fn local_micron_spawn_source_update_binds_uuid_and_current_address() {
    let page = LocalPage::new();
    let mut app = App::test_stub_at(page._directory.path().join("app"));
    let spawn = request(&mut app, &page);
    let mut opened = open(&app, prepare(&app, &spawn).unwrap().unwrap()).unwrap().unwrap();
    // Add a second member through normal navigation, then navigate it to the
    // same resource address. Sharing a URL does not share source admission.
    app.update(Action::OpenAddress("file:///sibling.mu".into()));
    let sibling = app.graph_runtimes.focused_member().unwrap();
    assert_ne!(spawn.0, sibling);
    let graph = app.graph_runtimes.graph_containing_member(sibling).unwrap();
    assert!(app.graph_runtimes.canvas_mut(graph).unwrap().navigate_member(sibling, &page.url));
    let source = opened.source.take().expect("successful real open returns typed source");
    app.apply_update(source);
    assert_eq!(app.content.fetched(spawn.0, &spawn.1).unwrap().body, SOURCE);
    assert!(app.content.fetched(sibling, &page.url).is_none());

    // Obtain another real result, then change its owner before delivery.
    // Use sibling's actual Toggle effect, whose cache remains empty.
    let sibling_spawn = spawn_from(app.update(Action::ToggleNodeContent));
    assert_eq!(sibling_spawn, (sibling, page.url.clone()));
    let mut sibling_opened = open(&app, prepare(&app, &sibling_spawn).unwrap().unwrap()).unwrap().unwrap();
    let source = sibling_opened.source.take().expect("source awaiting pure-fold acceptance");
    assert!(app.graph_runtimes.canvas_mut(graph).unwrap().navigate_member(sibling, "file:///newer.mu"));
    app.apply_update(source);
    assert!(app.content.fetched(sibling, &page.url).is_none());
    assert!(app.content.fetched(sibling, "file:///newer.mu").is_none());
    assert_eq!(app.content.fetched(spawn.0, &spawn.1).unwrap().body, SOURCE);
}

#[test]
fn local_micron_spawn_failed_controller_does_not_publish_prepared_source() {
    let page = LocalPage::new();
    let mut app = App::test_stub_at(page._directory.path().join("app"));
    let spawn = request(&mut app, &page);
    let prepared = prepare(&app, &spawn).unwrap().unwrap();
    let result = open_prepared_content_controller(
        &app, prepared,
        Arc::new(inker::SessionRegistry::<netrender::Scene>::new()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        super::super::ShellClock(std::time::Instant::now()),
    );
    assert!(result.is_err(), "actual registry failure never yields accepted source");
    assert!(app.content.fetched(spawn.0, &spawn.1).is_none());
}

#[test]
fn retained_clip_spawn_uses_held_document_through_standard_pelt_controller() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::test_stub_at(directory.path().join("app"));
    let address = crate::retained_clip::address(eidetic::ManifestId::from_hash(
        eidetic::Hash::of(b"retained clip host composition fixture"),
    ));
    // Retained clips request content in OpenAddress itself; toggling here
    // would close the actual request before host preparation can receive it.
    let spawn = spawn_from(app.update(Action::OpenAddress(address.clone())));
    assert_eq!(spawn.1, address);
    assert!(content_spawn_is_current(&app, spawn.0, &spawn.1));
    let document = inker::EngineDocument {
        address: address.clone(),
        title: Some("Retained café".into()),
        content_type: "application/vnd.mere.clip+json".into(),
        lang: None,
        provenance: inker::DocumentProvenance::for_engine(crate::retained_clip::ENGINE_ID, &address),
        trust: inker::DocumentTrustState::Unknown,
        diagnostics: vec![],
        navigation: Default::default(),
        blocks: vec![
            inker::Block::Heading {
                level: 1,
                spans: vec![inker::InlineSpan::Text("Retained café".into())],
            },
            inker::Block::Paragraph {
                spans: vec![inker::InlineSpan::Text("Selected café payload".into())],
            },
        ],
    };
    // A clip's host-resolved document must win over any fetched-body fallback.
    let fetched_body = "<html><title>Unrelated fetched body</title></html>";
    let fetched = crate::content::FetchedDocument {
        bytes: fetched_body.as_bytes().to_vec(),
        body: fetched_body.into(),
        content_type: Some("text/html".into()),
        effective_url: Some(address.clone()),
        acquired_at_ms: 1,
    };
    let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone"),
        view: None,
        node: None,
        address: address.clone(),
        content_type: fetched.content_type.clone(),
        pinned_engine: Some(crate::retained_clip::ENGINE_ID.into()),
    });
    assert_eq!(decision.engine_id, crate::retained_clip::ENGINE_ID);
    assert_eq!(dispatch_content_spawn(&decision.engine_id, false, &mut Vec::new(), spawn.0, &spawn.1), ContentSpawnDispatch::Session);
    let prepared = prepare_content_controller(
        &app, spawn.0, &spawn.1, &decision.engine_id,
        Some(fetched), Some(document.clone()), (480, 320),
    ).unwrap().unwrap();
    assert!(prepared.source.is_none(), "a held clip does not fabricate captured fetched source");
    assert!(prepared.config.request.body.is_some(), "the registered clip engine receives host-held JSON");
    assert_eq!(prepared.config.request.content_type.as_deref(), Some(document.content_type.as_str()));
    let engines = super::super::standard_content_engines();
    assert!(engines.contains(crate::retained_clip::ENGINE_ID));
    let mut opened = open_prepared_content_controller(
        &app, prepared, Arc::new(engines),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        super::super::ShellClock(std::time::Instant::now()),
    ).unwrap().unwrap();
    assert!(opened.source.is_none());
    assert_eq!(opened.controller.session().inspect().unwrap().title.as_deref(), Some("Retained café"));
    {
        let native = opened.controller.session_mut().as_any()
            .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
            .expect("the standard registered clip engine constructed the real retained session");
        assert_eq!(native.document_mut().document(), &document);
    }
    let scene = opened.controller.session_mut().frame(480, 320);
    assert!(scene.ops.iter().any(|op| matches!(op, netrender::SceneOp::GlyphRun(_))));
    assert!(app.content.fetched(spawn.0, &spawn.1).is_none());
}
