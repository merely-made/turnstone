// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn reserved_workspace_keeps_frame_and_chrome_at_full_window_size() {
    let app = App::test_stub();
    let window = Rect::full(1024, 600);
    let area = crate::chrome_view::workspace_area(app.shell_chrome_config(), true, 1024, 600);
    let plan = primary_content_plan(&app, &NodeDocuments::default(), 1024, 600);
    for kind in [SurfaceKind::Frame, SurfaceKind::Chrome] {
        assert_eq!(
            plan.iter()
                .find(|surface| surface.kind == kind)
                .unwrap()
                .rect,
            window,
        );
    }
    for surface in &plan {
        if matches!(
            surface.kind,
            SurfaceKind::Graph(_) | SurfaceKind::Pane(_) | SurfaceKind::Content(_)
        ) {
            assert!(surface.rect.y >= area.y);
            assert!(surface.rect.y + surface.rect.h <= area.y + area.h + 0.1);
        }
    }
}

#[test]
fn hidden_livery_animation_reaches_quiescence_without_redraw() {
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Clock(Arc<AtomicU64>);
    impl pelt_core::PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            self.0.load(Ordering::Relaxed) as f64
        }
    }
    let clock = Arc::new(AtomicU64::new(0));
    let request = SessionSpawnRequest::new("https://test.invalid/animation")
            .with_body("<html><head><style>@keyframes fade { from { opacity: 0.2; } to { opacity: 0.8; } } #a { animation: fade 1000ms linear; width: 100px; height: 100px; background: red; }</style></head><body><div id=a>fade</div></body></html>")
            .with_viewport(640, 480);
    let route = standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("hidden-animation"),
        view: None,
        node: None,
        address: request.address.clone(),
        content_type: None,
        pinned_engine: Some(inker::routing::ENGINE_GENET_LIVERY.into()),
    });
    let config = pelt_core::PeltControllerConfig::from_request(route.engine_id.clone(), request)
        .with_host_history();
    let mut controller = pelt_core::PeltController::new_shared(
        Arc::new(standard_content_engines()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        config,
        Clock(clock.clone()),
    )
    .expect("the Livery lane spawns");
    let _ = controller.frame(640, 480);
    assert!(
        !controller.session_mut().settled(),
        "the real CSS animation is initially active"
    );
    let content = pelt_core::PeltContent::from_controller(
        controller,
        pelt_core::PeltRoute {
            decision: route,
            source: pelt_core::PeltRouteSource::Automatic,
            state: pelt_core::PeltRouteState::Document,
        },
    );
    let mut contents = std::collections::HashMap::from([(1u8, content)]);
    clock.store(2000, Ordering::Relaxed);
    assert!(!crate::document_pump::pump_documents(&mut contents, &[]));
    assert!(
        contents
            .get_mut(&1)
            .unwrap()
            .document_mut()
            .unwrap()
            .session_mut()
            .settled(),
        "a hidden finite animation can finish without another frame"
    );
}

/// SC controller-level input over Turnstone's own engines: a host-history
/// controller hands a Spartan prompt's submission up as a POST the
/// submission conversation completes, and a link up as a navigation the
/// graph places. Keyboard focus and Enter reach the page through the same
/// neutral input, and nothing loads in place.
#[test]
fn a_document_controller_hands_links_and_submissions_up() {
    use inker::{
        SessionButtonState, SessionFocusDirection, SessionFormMethod, SessionInput, SessionKey,
        SessionModifiers,
    };
    let address = "spartan://capsule.test/";
    let decision = standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
        view: None,
        node: None,
        address: address.to_string(),
        content_type: None,
        pinned_engine: None,
    });
    let request = SessionSpawnRequest::new(address)
        .with_body("=: /sign Sign it\n=> /next Next\n")
        .with_viewport(640, 480);
    let config = pelt_core::PeltControllerConfig::from_request(decision.engine_id, request)
        .with_host_history();
    let mut controller = pelt_core::PeltController::new_shared(
        Arc::new(standard_content_engines()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        config,
        ShellClock(std::time::Instant::now()),
    )
    .expect("the Spartan lane spawns");
    let _ = controller.frame(640, 480);
    let enter = || SessionInput::Key {
        key: SessionKey::Enter,
        state: SessionButtonState::Pressed,
        modifiers: SessionModifiers::default(),
        repeat: false,
    };

    assert!(
        controller
            .input(SessionInput::FocusMove(SessionFocusDirection::Forward))
            .handled
    );
    let submitted = controller.input(enter());
    let submission = submitted
        .submission
        .expect("the prompt hands its submission up");
    assert_eq!(submission.method, SessionFormMethod::Post);
    assert_eq!(submission.action, "spartan://capsule.test/sign");
    assert_eq!(
        submitted.navigation, None,
        "a submission is not a navigation"
    );

    assert!(
        controller
            .input(SessionInput::FocusMove(SessionFocusDirection::Forward))
            .handled
    );
    let followed = controller.input(enter());
    let navigation = followed
        .navigation
        .expect("the link hands its navigation up");
    assert_eq!(navigation.request.address, "spartan://capsule.test/next");
    assert_eq!(controller.address(), address, "nothing loads in place");
}

/// Gate 1 of the smolweb browser gap analysis: Turnstone's default route
/// for a Gemini address must name a registered retained-session engine,
/// and that engine must lower real gemtext into paint plus a clickable
/// navigation target. The body is supplied at the spawn seam so the
/// receipt is deterministic and does not depend on a public capsule.
#[test]
fn gemini_route_lands_in_a_live_nematic_session() {
    let engines = standard_content_engines();
    let decision = standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
        view: None,
        node: None,
        address: "gemini://capsule.test/".to_string(),
        content_type: None,
        pinned_engine: None,
    });

    assert_eq!(decision.engine_id, inker::routing::ENGINE_NEMATIC_GEMTEXT);
    assert!(engines.contains(&decision.engine_id));

    let request = SessionSpawnRequest::new("gemini://capsule.test/")
        .with_body("# Turnstone Capsule\n\n=> /next Follow the next link\n")
        .with_viewport(640, 480);
    let mut session = engines
        .spawn(&decision.engine_id, &request)
        .expect("the registered Gemini lane spawns");
    let scene = session.frame(640, 480);
    assert!(
        scene
            .ops
            .iter()
            .any(|op| matches!(op, netrender::SceneOp::GlyphRun(_))),
        "gemtext text reaches the retained scene"
    );
    let report = session.inspect().expect("Nematic exposes structure");
    assert_eq!(report.title.as_deref(), Some("Turnstone Capsule"));
    assert_eq!(report.links, vec!["/next"]);

    let link = session.links().into_iter().next().expect("laid-out link");
    let [x, y, width, height] = link.rect;
    let SessionClick::Navigate(href) = session.click_at(x + width / 2.0, y + height / 2.0) else {
        panic!("the laid-out gemtext link must navigate");
    };

    let mut app = App::test_stub();
    let _ = app.update(Action::OpenAddress(
        "gemini://capsule.test/start".to_string(),
    ));
    let node = app
        .graph_runtimes
        .focused_member()
        .expect("opened capsule node");
    assert_eq!(
        content_link_target(&app, node, &href),
        "gemini://capsule.test/next"
    );
}

/// Scroll is a complete retained-document lane, not a gemtext fallback:
/// its shared route, remote fetcher and ScrollEngine must remain registered
/// together in the product host.
#[test]
fn scroll_route_lands_in_a_live_nematic_session() {
    let engines = standard_content_engines();
    let decision = standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
        view: None,
        node: None,
        address: "scroll://archive.test/reading.scroll".to_string(),
        content_type: Some("text/scroll".to_string()),
        pinned_engine: None,
    });

    assert_eq!(decision.engine_id, inker::routing::ENGINE_NEMATIC_SCROLL);
    assert!(engines.contains(&decision.engine_id));

    let request = SessionSpawnRequest::new("scroll://archive.test/reading.scroll")
        .with_content_type("text/scroll")
        .with_body(
            "# Scroll reading\n\
                 A protocol-faithful source keeps its own structure.\n\
                 => /reference Source record [Citation]\n",
        )
        .with_viewport(640, 480);
    let mut session = engines
        .spawn(&decision.engine_id, &request)
        .expect("the registered Scroll lane spawns");
    let scene = session.frame(640, 480);
    assert!(
        scene
            .ops
            .iter()
            .any(|op| matches!(op, netrender::SceneOp::GlyphRun(_))),
        "scrolltext reaches the retained scene"
    );
    let report = session.inspect().expect("Scroll exposes structure");
    assert_eq!(report.title.as_deref(), Some("Scroll reading"));
    assert_eq!(report.links, vec!["/reference"]);

    let link = session.links().into_iter().next().expect("laid-out link");
    let [x, y, width, height] = link.rect;
    let SessionClick::Navigate(href) = session.click_at(x + width / 2.0, y + height / 2.0) else {
        panic!("the laid-out Scroll link must navigate");
    };
    assert_eq!(href, "/reference");
}

#[test]
fn automatic_html_routes_to_the_registered_livery_session() {
    let engines = standard_content_engines();
    let decision = standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
        view: None,
        node: None,
        address: "https://example.test/".to_string(),
        content_type: Some("text/html".to_string()),
        pinned_engine: None,
    });

    assert_eq!(decision.engine_id, inker::routing::ENGINE_GENET_LIVERY);
    assert!(engines.contains(&decision.engine_id));
    assert!(!engines.contains(inker::routing::ENGINE_GENET_WEB));
}

#[test]
fn micron_file_uses_native_projection_and_preserves_source() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("reading.mu");
    let source = ">Heading\n---\nplain\n`[Local page`:/page/probe.mu]\n";
    std::fs::write(&path, source).unwrap();
    let address = url::Url::from_file_path(&path).unwrap().to_string();
    assert!(crate::nomadnet::is_micron_address(&address));
    let engines = standard_content_engines();
    let mut session = engines
        .spawn(
            crate::nomadnet::ENGINE_ID,
            &SessionSpawnRequest::new(&address).with_viewport(640, 480),
        )
        .expect("local Micron file loads through the registered native lane");
    assert!(
        session
            .frame(640, 480)
            .ops
            .iter()
            .any(|op| matches!(op, netrender::SceneOp::GlyphRun(_)))
    );
    let native = session
        .as_any()
        .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
        .unwrap();
    let document = native.document_mut().document();
    assert_eq!(
        document.provenance.source_kind.as_deref(),
        Some("nematic.micron")
    );
    assert!(document.content_type.is_empty());
    assert!(
        document
            .blocks
            .iter()
            .any(|block| matches!(block, inker::Block::Heading { level: 1, .. }))
    );
    assert!(
        document
            .blocks
            .iter()
            .any(|block| matches!(block, inker::Block::Rule))
    );
    assert!(document.blocks.iter().any(
        |block| matches!(block, inker::Block::Badge { text } if text.contains("Micron preview"))
    ));
    assert!(document.blocks.iter().any(|block| matches!(block, inker::Block::Paragraph { spans } if inker::inline_text(spans) == "Local page")));
    assert!(document.outgoing_links().is_empty());
    assert!(
        session.links().is_empty(),
        "file aliases remain inert without site authority"
    );
    assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
}

#[test]
fn reader_pin_uses_held_html_and_switching_back_restores_original_rendering() {
    let engines = standard_content_engines();
    assert!(engines.contains(inker::routing::ENGINE_GENET_READER));
    let source = "<html><head><title>Original Page</title></head><body><nav>Site chrome</nav>\
            <main><h1>Reader Heading</h1><p>This substantial article paragraph proves the shared \
            reader lane while retaining the original source bytes.</p></main></body></html>";
    let retained = source.to_string();
    let reader_decision = standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
        view: None,
        node: None,
        address: "https://example.test/story".to_string(),
        content_type: Some("text/html".to_string()),
        pinned_engine: Some(inker::routing::ENGINE_GENET_READER.to_string()),
    });
    assert_eq!(
        reader_decision.engine_id,
        inker::routing::ENGINE_GENET_READER
    );
    let request = SessionSpawnRequest::new("https://example.test/story")
        .with_body(source)
        .with_content_type("text/html")
        .with_viewport(640, 480);
    let reader = engines
        .spawn(&reader_decision.engine_id, &request)
        .expect("reader spawns from held bytes");
    let reader_report = reader.inspect().expect("reader inspection");
    assert_eq!(reader_report.title.as_deref(), Some("Reader Heading"));
    assert!(reader_report.lineage.is_some());

    let original_decision = standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
        view: None,
        node: None,
        address: "https://example.test/story".to_string(),
        content_type: Some("text/html".to_string()),
        pinned_engine: Some(inker::routing::ENGINE_GENET_LIVERY.to_string()),
    });
    let original = engines
        .spawn(&original_decision.engine_id, &request)
        .expect("original renderer respawns from the same held bytes");
    assert_eq!(
        original.inspect().and_then(|report| report.title),
        Some("Original Page".to_string())
    );
    assert_eq!(
        retained, source,
        "reader extraction does not mutate held source"
    );
}

/// The one-state-N-windows invariant (rung 7): two windows on one graph
/// hold DISTINCT cameras over shared positions. Install/stash through the
/// canvas's viewport seam keeps a pan in one lens out of the other.
#[test]
fn lens_viewports_stay_distinct() {
    let mut canvas = mere::canvas::Canvas::with_sample_graph();
    canvas.resize(800, 600);
    let a = canvas.viewport();
    // Drive "window B": install, pan, stash.
    canvas.set_viewport(a);
    canvas.wheel(0.0, 240.0);
    let b = canvas.viewport();
    // Restore "window A".
    canvas.set_viewport(a);
    assert_ne!(a, b, "B's wheel moved B's viewport (inertia counts)");
    assert_eq!(canvas.viewport(), a, "A's viewport is untouched");
}

/// The drop decode: a real PNG round-trips to a data-URI; a non-image
/// file declines (and so becomes a node instead of a sprite).
#[test]
fn dropped_files_classify_by_decodability() {
    let dir = std::env::temp_dir().join(format!("turnstone-drop-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let png_path = dir.join("drop.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]))
        .save(&png_path)
        .unwrap();
    let (uri, hull) = decode_sprite(&png_path).expect("a png decodes");
    assert!(uri.starts_with("data:image/png;base64,"));
    assert!(hull.len() >= 3, "an opaque png traces a collider hull");
    let txt_path = dir.join("drop.txt");
    std::fs::write(&txt_path, "not an image").unwrap();
    assert!(decode_sprite(&txt_path).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
