// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn settled_timer_deadline_survives_early_events_and_uses_configured_runnable_cadence() {
    let start = std::time::Instant::now();
    let work = pelt_core::PeltDocumentWork {
        content: ContentKey::Node(uuid::Uuid::from_u128(7)),
        identity: pelt_core::PeltSessionIdentity {
            instance_id: 41,
            generation: 2,
        },
        work: inker::session_engine::SessionPendingWork::timer(std::time::Duration::from_secs(1)),
    };
    let mut schedule = DocumentClock::new(std::time::Duration::from_millis(25));
    schedule.observe_work(false, [work.clone()], start);
    let timer = start + std::time::Duration::from_secs(1);
    for milliseconds in [0, 1, 50, 250, 999] {
        let early = start + std::time::Duration::from_millis(milliseconds);
        assert_eq!(schedule.deadline(early), Some(timer));
        assert!(!schedule.poll_due(early));
        assert!(schedule.due_work(early).is_empty());
    }
    assert!(schedule.poll_due(timer));
    assert_eq!(schedule.due_work(timer), [work.clone()]);
    schedule.observe_work(false, [], timer);
    assert_eq!(schedule.deadline(timer), None);
    let mut runnable = work.clone();
    runnable.work.microtasks = true;
    schedule.observe_work(false, [runnable], timer);
    assert_eq!(
        schedule.deadline(timer),
        Some(timer + std::time::Duration::from_millis(25))
    );
}

#[test]
fn settled_timer_turn_mutates_its_actual_receiver_and_queues_only_current_visible_window() {
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Clock(Arc<AtomicU64>);
    impl pelt_core::PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            self.0.load(Ordering::Relaxed) as f64
        }
    }
    // A hosted timer contract model. This verifies host scheduling and
    // receiver custody, not conformance of any optional JS interpreter.
    struct Timer {
        now: f64,
        fired: bool,
    }
    impl inker::DocumentSession<String> for Timer {
        fn frame(&mut self, _: u32, _: u32) -> String {
            if self.fired {
                "timer fired"
            } else {
                "before timer"
            }
            .into()
        }
        fn pump(&mut self, now: f64) {
            self.now = now;
            self.fired |= now >= 1000.0;
        }
        fn pending_work(&mut self) -> inker::session_engine::SessionPendingWork {
            if self.fired {
                inker::session_engine::SessionPendingWork::idle()
            } else {
                inker::session_engine::SessionPendingWork::timer(
                    std::time::Duration::from_secs_f64((1000.0 - self.now).max(0.0) / 1000.0),
                )
            }
        }
        fn as_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
        fn as_any_ref(&self) -> &dyn std::any::Any {
            self
        }
        fn scroll_by(&mut self, _: f32, _: f32) -> bool {
            false
        }
        fn scroll_for_key(&mut self, _: inker::SessionScrollKey) -> bool {
            false
        }
        fn click_at(&mut self, _: f32, _: f32) -> inker::SessionClick {
            inker::SessionClick::Miss
        }
        fn links(&self) -> Vec<inker::SessionLink> {
            Vec::new()
        }
    }
    let key = ContentKey::Node(uuid::Uuid::from_u128(7));
    let now = Arc::new(AtomicU64::new(0));
    let request = inker::SessionSpawnRequest::new("https://fixture.test/timer");
    let decision = super::super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("host-timer-contract"),
        view: None,
        node: None,
        address: request.address.clone(),
        content_type: None,
        pinned_engine: None,
    });
    let controller = PeltController::from_session_shared(
        Arc::new(inker::SessionRegistry::new()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        PeltControllerConfig::from_request("timer-contract", request),
        Clock(now.clone()),
        Box::new(Timer {
            now: 0.0,
            fired: false,
        }),
    );
    let mut pool = PeltControllerPool::<ContentKey, u8, String>::new();
    pool.insert(
        key,
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision,
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        ),
    );
    let mut visible = pelt_core::PeltPlacement {
        content: key,
        window: 7,
        rect: pelt_core::WorkspaceRect::new(0.0, 0.0, 640.0, 480.0),
        visible: true,
        focused: false,
        scale_factor: 1.0,
    };
    let mut hidden = visible.clone();
    hidden.window = 8;
    hidden.visible = false;
    pool.set_placements([(1, visible.clone()), (2, hidden)]);
    assert!(
        !pool.pump(),
        "script-visible completion ignores future timers"
    );
    let mut schedule = DocumentClock::new(std::time::Duration::from_millis(25));
    let start = std::time::Instant::now();
    schedule.observe_work(false, pool.document_work(), start);
    assert!(
        schedule.deadline(start).is_some(),
        "settled timer still owns a wake"
    );
    // A station move transfers the same timer receiver's paint admission.
    visible.window = 9;
    pool.set_placements([(1, visible)]);
    let deadline = start + std::time::Duration::from_secs(1);
    assert!(schedule.poll_due(deadline));
    let due = schedule.due_work(deadline);
    let requested: Vec<_> = [7, 8, 9, 10]
        .into_iter()
        .filter(|window| {
            due.iter()
                .any(|work| pool.window_has_document_work(*window, work))
        })
        .collect();
    assert_eq!(requested, [9]);
    now.store(1000, Ordering::Relaxed);
    let pending = pool.pump();
    assert!(!pending);
    schedule.observe_work(pending, pool.document_work(), deadline);
    assert_eq!(schedule.deadline(deadline), None);
    let pelt_core::PeltLayer::Document(frame) = pool.frame(&1).unwrap() else {
        panic!("timer document");
    };
    assert_eq!(frame, "timer fired");
}

#[test]
fn placements_carry_the_windows_layout_scale_over_the_physical_plan() {
    let surface = Surface {
        id: SurfaceId(3),
        kind: SurfaceKind::Content(uuid::Uuid::from_u128(3)),
        rect: Rect::new(20.0, 30.0, 640.0, 480.0),
    };
    for scale in [2.0, 0.75] {
        let placement = HostPlacement::at(surface, 1, false, scale);
        assert_eq!(
            placement.surface.rect, surface.rect,
            "the plan stays physical"
        );
        assert_eq!(placement.scale_factor, scale as f32);
    }
    assert_eq!(
        HostPlacement::at(surface, 1, false, f64::NAN).scale_factor,
        1.0
    );
}

#[test]
fn repeated_real_viewports_change_projection_revision_even_when_returning_to_the_first_size() {
    for (engine, address, body) in [
        (
            inker::routing::ENGINE_NEMATIC_GEMTEXT,
            "gemini://fixture.test/",
            "=> /next A long named link that needs several lines at a narrow viewport and fewer lines at a wide viewport\n",
        ),
        (
            inker::routing::ENGINE_GENET_LIVERY,
            "https://fixture.test/",
            "<html><body><a href='/next'>A long named link that needs several lines at a narrow viewport and fewer lines at a wide viewport</a></body></html>",
        ),
    ] {
        let mut controller = PeltController::new_shared(
            Arc::new(super::super::super::standard_content_engines()),
            Arc::new(inker::SurfaceEngineRegistry::new()),
            PeltControllerConfig::from_request(
                engine,
                inker::SessionSpawnRequest::new(address)
                    .with_body(body)
                    .with_viewport(180, 300),
            )
            .with_host_history(),
            super::super::super::ShellClock(std::time::Instant::now()),
        )
        .unwrap();
        let _ = controller.frame(180, 300);
        let first = controller.accessibility_projection().unwrap();
        let _ = controller.frame(180, 300);
        let stable = controller.accessibility_projection().unwrap();
        assert_eq!(
            first.revision(),
            stable.revision(),
            "{engine}: unchanged viewport remains current"
        );
        let _ = controller.frame(640, 300);
        let wide = controller.accessibility_projection().unwrap();
        let _ = controller.frame(180, 300);
        let returned = controller.accessibility_projection().unwrap();
        eprintln!(
            "{engine}: narrow={} same={} wide={} narrow-again={}",
            first.revision(),
            stable.revision(),
            wide.revision(),
            returned.revision()
        );
        assert_ne!(
            first.revision(),
            wide.revision(),
            "{engine}: changed geometry retires prior revision"
        );
        assert_ne!(
            first.revision(),
            returned.revision(),
            "{engine}: restoring size does not restore revision"
        );
        let decision =
            super::super::super::standard_route_policy().route(&inker::EngineRouteRequest {
                workspace_id: inker::WorkspaceRouteId::new("real-appearance-a11y"),
                view: None,
                node: None,
                address: address.into(),
                content_type: None,
                pinned_engine: Some(engine.into()),
            });
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
        pool.set_placements([
            (
                1,
                pelt_core::PeltPlacement {
                    content: 1,
                    window: 0,
                    rect: pelt_core::WorkspaceRect::new(0., 0., 180., 300.),
                    visible: true,
                    focused: true,
                    scale_factor: 1.,
                },
            ),
            (
                2,
                pelt_core::PeltPlacement {
                    content: 1,
                    window: 1,
                    rect: pelt_core::WorkspaceRect::new(0., 0., 640., 300.),
                    visible: true,
                    focused: false,
                    scale_factor: 1.,
                },
            ),
        ]);
        let narrow = pool.accessibility_projection(&1).unwrap();
        let wide = pool.accessibility_projection(&2).unwrap();
        let action = |snapshot: &pelt_core::PeltPlacementA11y<u8, u8>, action| {
            inker::DocumentA11yActionRequest {
                revision: snapshot.projection().revision(),
                target: snapshot
                    .projection()
                    .nodes()
                    .iter()
                    .find(|node| node.role == inker::DocumentA11yRole::Link)
                    .unwrap()
                    .id,
                action,
                data: None,
            }
        };
        let focused = pool.dispatch_accessibility_action(
            &narrow,
            &action(&narrow, inker::DocumentA11yAction::Focus),
        );
        assert!(
            focused.handled,
            "{engine}: first placement must remain actionable after second layout"
        );
        assert!(
            focused.navigation.is_none(),
            "{engine}: Focus is nonactivating"
        );
        assert!(
            !pool
                .dispatch_accessibility_action(
                    &wide,
                    &action(&wide, inker::DocumentA11yAction::Click)
                )
                .handled,
            "{engine}: Focus changed source state, so earlier observations retire"
        );
        let _narrow = pool.accessibility_projection(&1).unwrap();
        let wide = pool.accessibility_projection(&2).unwrap();
        let _ = pool.frame(&1);
        let clicked = pool
            .dispatch_accessibility_action(&wide, &action(&wide, inker::DocumentA11yAction::Click));
        assert_eq!(
            clicked
                .navigation
                .as_ref()
                .map(|n| n.request.address.as_str()),
            Some(format!("{address}next").as_str()),
            "{engine}: second placement restores its actual hit geometry"
        );
        assert_eq!(
            pool.content(&1).unwrap().document().unwrap().address(),
            address,
            "host history never navigates the controller in place"
        );
        assert!(
            !pool
                .dispatch_accessibility_action(
                    &wide,
                    &action(&wide, inker::DocumentA11yAction::Click)
                )
                .handled,
            "one captured activation cannot be replayed"
        );
    }
}

#[test]
fn terminal_document_turn_paints_its_last_change_without_polling_quiet_windows() {
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Clock(Arc<AtomicU64>);
    impl pelt_core::PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            self.0.load(Ordering::Relaxed) as f64
        }
    }
    let now = Arc::new(AtomicU64::new(0));
    let request = inker::SessionSpawnRequest::new("https://fixture.test/terminal-animation")
            .with_body("<html><head><style>@keyframes fade { from { opacity:0.2; } to { opacity:0.8; } } #a { animation:fade 1000ms linear; width:100px; height:100px; background:red; }</style></head><body><div id=a>fade</div></body></html>")
            .with_viewport(640, 480);
    let decision = super::super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("terminal-pool-animation"),
        view: None,
        node: None,
        address: request.address.clone(),
        content_type: None,
        pinned_engine: Some(inker::routing::ENGINE_GENET_LIVERY.into()),
    });
    let controller = PeltController::new_shared(
        Arc::new(super::super::super::standard_content_engines()),
        Arc::new(inker::SurfaceEngineRegistry::new()),
        PeltControllerConfig::from_request(decision.engine_id.clone(), request).with_host_history(),
        Clock(now.clone()),
    )
    .unwrap();
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
    let visible = pelt_core::PeltPlacement {
        content: 1,
        window: 7,
        rect: pelt_core::WorkspaceRect::new(0.0, 0.0, 640.0, 480.0),
        visible: true,
        focused: false,
        scale_factor: 1.0,
    };
    let mut hidden = visible.clone();
    hidden.window = 8;
    hidden.visible = false;
    pool.set_placements([(1, visible), (2, hidden)]);
    let pelt_core::PeltLayer::Document(initial) = pool.frame(&1).unwrap() else {
        panic!("real Livery document must produce a Scene");
    };
    assert!(pool.pump());
    let mut schedule = DocumentClock::new(std::time::Duration::from_millis(25));
    schedule.observe(true);
    let start = std::time::Instant::now();
    let deadline = schedule.deadline(start).unwrap();
    assert!(schedule.poll_due(deadline));
    now.store(2000, Ordering::Relaxed);
    let mut requested = std::collections::BTreeSet::new();
    pump_document_turn(
        &mut pool,
        |pool| {
            requested.extend(
                [7, 8, 9]
                    .into_iter()
                    .filter(|window| pool.window_needs_frame(*window)),
            );
        },
        |pool| schedule.observe(pool.pump()),
    );
    assert!(
        !pool.window_needs_frame(7),
        "terminal pump retired pending work"
    );
    assert_eq!(
        requested,
        [7].into_iter().collect(),
        "terminal paint belongs only to the admitted window"
    );
    let pelt_core::PeltLayer::Document(final_frame) = pool.frame(&1).unwrap() else {
        panic!("real Livery document must produce a Scene");
    };
    assert_ne!(
        initial.snapshot_json(),
        final_frame.snapshot_json(),
        "the settled turn changed actual painted animation state"
    );
    assert_eq!(schedule.deadline(deadline), None);
}
