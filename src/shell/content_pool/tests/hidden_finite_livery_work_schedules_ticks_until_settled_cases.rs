// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn hidden_finite_livery_work_schedules_ticks_until_settled_without_paints() {
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Clock(Arc<AtomicU64>);
    impl pelt_core::PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            self.0.load(Ordering::Relaxed) as f64
        }
    }
    let now = Arc::new(AtomicU64::new(0));
    let engines = Arc::new(super::super::super::standard_content_engines());
    let surface_engines = Arc::new(inker::SurfaceEngineRegistry::new());
    let request=inker::SessionSpawnRequest::new("https://fixture.test/animation")
            .with_body("<html><head><style>@keyframes fade { from { opacity:0.2; } to { opacity:0.8; } } #a { animation:fade 1000ms linear; width:100px; height:100px; background:red; }</style></head><body><div id=a>fade</div></body></html>")
            .with_viewport(640,480);
    let decision = super::super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("hidden-pool-animation"),
        view: None,
        node: None,
        address: request.address.clone(),
        content_type: None,
        pinned_engine: Some(inker::routing::ENGINE_GENET_LIVERY.into()),
    });
    let mut controller = PeltController::new_shared(
        engines,
        surface_engines,
        PeltControllerConfig::from_request(decision.engine_id.clone(), request).with_host_history(),
        Clock(now.clone()),
    )
    .unwrap();
    let _ = controller.frame(640, 480);
    assert!(!controller.session_mut().settled());
    let mut pool = PeltControllerPool::<u8, u8, Scene>::new();
    pool.insert(
        1,
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision,
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        ),
    );
    pool.set_placements([]);
    let mut schedule = DocumentClock::new(std::time::Duration::from_millis(25));
    let start = std::time::Instant::now();
    let pending = pool.pump();
    assert!(pending, "the real finite animation has more clock work");
    assert!(
        !pool.visible_needs_frame(),
        "hidden work must not request paint"
    );
    schedule.observe(pending);
    let deadline = schedule
        .deadline(start)
        .expect("hidden pending work owns an event-loop wake");
    assert_eq!(deadline, start + std::time::Duration::from_millis(25));
    assert!(!schedule.poll_due(start));
    assert!(schedule.poll_due(deadline));
    now.store(2000, Ordering::Relaxed);
    schedule.observe(pool.pump());
    assert!(!pool.visible_needs_frame());
    assert!(
        pool.content_mut(&1)
            .unwrap()
            .document_mut()
            .unwrap()
            .session_mut()
            .settled()
    );
    assert_eq!(
        schedule.deadline(deadline),
        None,
        "quiescence retires the document wake independently of surfaces"
    );
}

#[test]
fn reader_placements_share_source_and_retain_independent_scroll_after_hide() {
    let engines = Arc::new(super::super::super::standard_content_engines());
    let surfaces = Arc::new(inker::SurfaceEngineRegistry::new());
    let epoch = std::time::Instant::now();
    let body = format!("<html><body><main><h1>Article</h1><p><a href='/next'>Reader continuation</a> offers substantial introductory article content.</p>{}</main></body></html>",
            "<p>This is substantial reader article content with enough words to retain and scroll independently.</p>".repeat(150));
    let request = inker::SessionSpawnRequest::new("https://fixture.test/story")
        .with_body(body)
        .with_content_type("text/html")
        .with_viewport(640, 480);
    let decision = super::super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("pool-reader-test"),
        view: None,
        node: None,
        address: request.address.clone(),
        content_type: request.content_type.clone(),
        pinned_engine: Some(inker::routing::ENGINE_GENET_READER.into()),
    });
    let controller = PeltController::new_shared(
        engines.clone(),
        surfaces.clone(),
        PeltControllerConfig::from_request(decision.engine_id.clone(), request).with_host_history(),
        super::super::super::ShellClock(epoch),
    )
    .unwrap();
    let node = uuid::Uuid::from_u128(1);
    let mut contents = NodeDocuments::default();
    contents.insert(
        node,
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision,
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        ),
    );
    let placements = || {
        vec![
            HostPlacement {
                surface: Surface {
                    id: SurfaceId(11),
                    kind: SurfaceKind::Content(node),
                    rect: Rect::new(0., 0., 640., 400.),
                },
                window: 0,
                focused: true,
                scale_factor: 2.,
                reading_preferences: Default::default(),
                reading_anchor: None,
                color_mode: mere_document_lanes::ModeProfile::LIGHT,
            },
            HostPlacement {
                surface: Surface {
                    id: SurfaceId(12),
                    kind: SurfaceKind::Content(node),
                    rect: Rect::new(0., 0., 640., 300.),
                },
                window: 1,
                focused: false,
                scale_factor: 1.,
                reading_preferences: tabard::ReadingPreferences {
                    text_size: 20,
                    column_em: 32,
                },
                reading_anchor: None,
                color_mode: mere_document_lanes::ModeProfile::DARK,
            },
        ]
    };
    contents.sync_placements(placements(), engines.clone(), surfaces.clone(), epoch);
    let first = contents
        .pool
        .placement(&SurfaceId(11))
        .expect("primary Reader appearance admitted")
        .content;
    let second = contents
        .pool
        .placement(&SurfaceId(12))
        .expect("lens Reader appearance admitted")
        .content;
    assert_ne!(
        first, second,
        "independent appearances cannot share mutable controller state"
    );
    assert!(Arc::ptr_eq(
        &reader(&contents, &first).source_document(),
        &reader(&contents, &second).source_document()
    ));
    // Generic serif/sans faces can already cover the arrow on Linux.
    // Request the supplied family when checking its exact bytes, keeping
    // this registration check independent of system font availability.
    contents
        .pool
        .content_mut(&first)
        .unwrap()
        .document_mut()
        .unwrap()
        .session_mut()
        .as_any()
        .downcast_mut::<mere_document_lanes::ReaderDocumentSession>()
        .unwrap()
        .set_font_family("\"Noto Sans Math\"");
    let Some(pelt_core::PeltLayer::Document(scene)) = contents.pool.frame(&SurfaceId(11)) else {
        panic!("the reading appearance must paint");
    };
    assert!(
        scene
            .iter_glyph_runs()
            .all(|run| run.glyphs.iter().all(|glyph| glyph.id != 0)),
        "the same-scheme link adornment has a supplied symbol face"
    );
    assert!(
        scene
            .fonts
            .iter()
            .any(|font| font.data.as_ref() == READING_SYMBOL_FONT)
    );
    assert!(contents.pool.frame(&SurfaceId(12)).is_some());
    assert_eq!(
        reader(&contents, &first).viewport(),
        (320, 200),
        "a 2x window lays its physical 640x400 plan rect out in layout pixels"
    );
    assert_eq!(
        reader(&contents, &first).reading_preferences(),
        Default::default()
    );
    assert_eq!(
        reader(&contents, &second).reading_preferences(),
        tabard::ReadingPreferences {
            text_size: 20,
            column_em: 32
        }
    );
    assert_eq!(reader(&contents, &second).viewport(), (640, 300));
    assert_eq!(
        reader(&contents, &first).color_mode(),
        mere_document_lanes::ModeProfile::LIGHT
    );
    assert_eq!(
        reader(&contents, &second).color_mode(),
        mere_document_lanes::ModeProfile::DARK,
        "each appearance derives its colours under its placement's theme mode"
    );
    let first_projection = contents
        .pool
        .accessibility_projection(&SurfaceId(11))
        .unwrap();
    let link = first_projection
        .projection()
        .nodes()
        .iter()
        .find(|node| node.role == inker::DocumentA11yRole::Link)
        .unwrap()
        .id;
    let focused = contents.pool.dispatch_accessibility_action(
        &first_projection,
        &inker::DocumentA11yActionRequest {
            revision: first_projection.projection().revision(),
            target: link,
            action: inker::DocumentA11yAction::Focus,
            data: None,
        },
    );
    assert!(focused.handled && focused.navigation.is_none());
    assert!(
        contents
            .pool
            .accessibility_projection(&SurfaceId(11))
            .unwrap()
            .projection()
            .node(link)
            .unwrap()
            .state
            .focused
    );
    assert!(
        contents
            .pool
            .accessibility_projection(&SurfaceId(12))
            .unwrap()
            .projection()
            .nodes()
            .iter()
            .all(|node| !node.state.focused)
    );
    contents.pool.input(
        &SurfaceId(11),
        pelt_core::PeltInput::Wheel {
            position: inker::PhysicalPosition { x: 10., y: 10. },
            delta: (0., 120.),
            modifiers: inker::SessionModifiers::default(),
        },
    );
    let scroll = reader(&contents, &first).scroll_y();
    assert!(scroll > 0.);
    assert_eq!(reader(&contents, &second).scroll_y(), 0.);
    let identity = contents.pool.content(&first).unwrap().session_identity();
    let admission = contents.pool.placement_generation(&SurfaceId(11)).unwrap();
    contents.sync_placements(Vec::new(), engines.clone(), surfaces.clone(), epoch);
    assert!(!contents.pool.admits(&SurfaceId(11), admission));
    contents.sync_placements(placements(), engines.clone(), surfaces.clone(), epoch);
    assert_eq!(
        contents.pool.content(&first).unwrap().session_identity(),
        identity
    );
    assert_eq!(reader(&contents, &first).scroll_y(), scroll);
    assert_ne!(
        contents.pool.placement_generation(&SurfaceId(11)),
        Some(admission)
    );
    contents.pool.input(
        &SurfaceId(11),
        pelt_core::PeltInput::Wheel {
            position: inker::PhysicalPosition { x: 10., y: 10. },
            delta: (0., -10000.),
            modifiers: Default::default(),
        },
    );
    let reopened = contents
        .pool
        .accessibility_projection(&SurfaceId(11))
        .unwrap();
    assert!(
        reopened.projection().node(link).unwrap().state.focused,
        "appearance retains focus across hide/reopen"
    );
    let old_source = reader(&contents, &first).source_document();
    let canonical = contents.get(&node).unwrap();
    let old = canonical.document().unwrap();
    let request = old.request().clone();
    let controller = PeltController::new_shared(
        engines.clone(),
        surfaces.clone(),
        PeltControllerConfig::from_request(old.engine_id(), request).with_host_history(),
        super::super::super::ShellClock(epoch),
    )
    .unwrap();
    let route = canonical.route().clone();
    let mut retired = contents
        .insert(node, PeltContent::from_controller(controller, route))
        .unwrap();
    assert!(matches!(
        retired
            .cue_surface_mut()
            .unwrap()
            .input(inker::webcue::Input::Document(inker::SessionInput::Text(
                "late".into()
            ))),
        Err(inker::webcue::Error::Retired)
    ));
    contents.sync_placements(placements(), engines, surfaces, epoch);
    assert!(
        !Arc::ptr_eq(&old_source, &reader(&contents, &first).source_document()),
        "canonical replacement retires prior source custody"
    );
    assert!(
        !contents
            .pool
            .dispatch_accessibility_action(
                &reopened,
                &inker::DocumentA11yActionRequest {
                    revision: reopened.projection().revision(),
                    target: link,
                    action: inker::DocumentA11yAction::Focus,
                    data: None,
                }
            )
            .handled,
        "replacement refuses the earlier appearance admission"
    );
}
