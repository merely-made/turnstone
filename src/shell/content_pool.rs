// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Host identities and Reader appearance custody over the shared Pelt pool.
use crate::surface::{Surface, SurfaceId};
use netrender::Scene;
use pelt_core::{PeltContent, PeltControllerPool};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ContentKey {
    Node(uuid::Uuid),
    Reader {
        node: uuid::Uuid,
        appearance: SurfaceId,
    },
}

pub(super) struct HostPlacement {
    pub surface: Surface,
    pub window: u64,
    pub focused: bool,
    pub scale_factor: f32,
}

impl HostPlacement {
    pub(super) fn for_focus(
        surface: Surface,
        window: u64,
        active: bool,
        focus: crate::surface::FocusTarget,
        native_scale: f64,
    ) -> Self {
        Self::physical(
            surface,
            window,
            active
                && matches!(focus, crate::surface::FocusTarget::Content { node, appearance }
                if surface.kind == crate::surface::SurfaceKind::Content(node) && surface.id == appearance),
            native_scale,
        )
    }

    /// Surface plans already use physical pixels. Native display scale is
    /// intentionally observed here without applying it again to content.
    pub(super) fn physical(
        surface: Surface,
        window: u64,
        focused: bool,
        _native_scale: f64,
    ) -> Self {
        Self {
            surface,
            window,
            focused,
            scale_factor: 1.0,
        }
    }
}

#[derive(Default)]
pub(crate) struct NodeDocuments {
    pub(super) pool: PeltControllerPool<ContentKey, SurfaceId, Scene>,
}

impl NodeDocuments {
    pub fn get(&self, node: &uuid::Uuid) -> Option<&PeltContent<Scene>> {
        self.pool.content(&ContentKey::Node(*node))
    }
    pub fn get_mut(&mut self, node: &uuid::Uuid) -> Option<&mut PeltContent<Scene>> {
        self.pool.content_mut(&ContentKey::Node(*node))
    }
    pub fn contains_key(&self, node: &uuid::Uuid) -> bool {
        self.get(node).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&uuid::Uuid, &PeltContent<Scene>)> {
        self.pool.iter().filter_map(|(key, content)| match key {
            ContentKey::Node(node) => Some((node, content)),
            _ => None,
        })
    }
    pub fn values(&self) -> impl Iterator<Item = &PeltContent<Scene>> {
        self.iter().map(|(_, content)| content)
    }
    pub fn insert(
        &mut self,
        node: uuid::Uuid,
        content: PeltContent<Scene>,
    ) -> Option<PeltContent<Scene>> {
        self.clear_appearances(node);
        self.pool.insert(ContentKey::Node(node), content)
    }
    pub fn remove(&mut self, node: &uuid::Uuid) -> Option<PeltContent<Scene>> {
        self.clear_appearances(*node);
        self.pool.remove_content(&ContentKey::Node(*node))
    }
    pub fn clear(&mut self) {
        let keys: Vec<_> = self.pool.keys().copied().collect();
        for key in keys {
            self.pool.remove_content(&key);
        }
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&uuid::Uuid, &PeltContent<Scene>) -> bool) {
        let removed: Vec<_> = self
            .iter()
            .filter_map(|(node, content)| (!keep(node, content)).then_some(*node))
            .collect();
        for node in removed {
            self.remove(&node);
        }
    }
    pub(super) fn retain_appearances(
        &mut self,
        mut keep: impl FnMut(uuid::Uuid, SurfaceId) -> bool,
    ) {
        let removed: Vec<_> = self
            .pool
            .keys()
            .filter_map(|key| match *key {
                ContentKey::Reader { node, appearance } if !keep(node, appearance) => Some(*key),
                _ => None,
            })
            .collect();
        for key in removed {
            self.pool.remove_content(&key);
        }
    }
    pub(super) fn clear_appearances(&mut self, node: uuid::Uuid) {
        self.retain_appearances(|known, _| known != node);
    }
    pub(super) fn appearance(
        &self,
        node: uuid::Uuid,
        appearance: SurfaceId,
    ) -> Option<&PeltContent<Scene>> {
        self.pool
            .content(&ContentKey::Reader { node, appearance })
            .or_else(|| self.get(&node))
    }
    pub(super) fn sync_placements(
        &mut self,
        placements: Vec<HostPlacement>,
        engines: Arc<inker::SessionRegistry<Scene>>,
        surface_engines: Arc<inker::SurfaceEngineRegistry>,
        epoch: std::time::Instant,
    ) {
        let mut arranged = Vec::new();
        for host in placements {
            let crate::surface::SurfaceKind::Content(node) = host.surface.kind else {
                continue;
            };
            let Some(canonical) = self.get(&node) else {
                continue;
            };
            let mut key = ContentKey::Node(node);
            if let Some(controller) = canonical.document()
                && let Some(reader) = controller
                    .session()
                    .as_any_ref()
                    .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()
            {
                key = ContentKey::Reader {
                    node,
                    appearance: host.surface.id,
                };
                let source = reader.source_document();
                let current = self
                    .pool
                    .content(&key)
                    .and_then(PeltContent::document)
                    .and_then(|controller| {
                        controller
                            .session()
                            .as_any_ref()
                            .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()
                    })
                    .is_some_and(|retained| Arc::ptr_eq(&source, &retained.source_document()));
                if !current {
                    let session = reader.new_appearance((1, 1));
                    let config = pelt_core::PeltControllerConfig::from_request(
                        controller.engine_id(),
                        controller.request().clone(),
                    )
                    .with_host_history();
                    let route = canonical.route().clone();
                    let controller = pelt_core::PeltController::from_session_shared(
                        engines.clone(),
                        surface_engines.clone(),
                        config,
                        super::ShellClock(epoch),
                        Box::new(session),
                    );
                    self.pool
                        .insert(key, PeltContent::from_controller(controller, route));
                }
            }
            let r = host.surface.rect;
            arranged.push((
                host.surface.id,
                pelt_core::PeltPlacement {
                    content: key,
                    window: host.window,
                    rect: pelt_core::WorkspaceRect::new(r.x, r.y, r.w, r.h),
                    visible: true,
                    focused: host.focused,
                    scale_factor: host.scale_factor,
                },
            ));
        }
        self.pool.set_placements(arranged);
    }
}

/// Request admitted pending windows before advancing a due document turn too:
/// its terminal pump may change the painted state while retiring pending work.
pub(super) fn pump_document_turn<T>(
    host: &mut T,
    mut request_redraws: impl FnMut(&T),
    pump: impl FnOnce(&mut T),
) {
    request_redraws(host);
    pump(host);
    request_redraws(host);
}

/// Event-loop document work shares the existing configured polling cadence;
/// pending work schedules ticks independently of window paint requests.
pub(super) struct DocumentClock {
    clock: super::surface_poll::SurfacePollClock,
    pending: bool,
    timers: Vec<(pelt_core::PeltDocumentWork<ContentKey>, std::time::Instant)>,
}
impl DocumentClock {
    pub(super) fn new(interval: std::time::Duration) -> Self {
        Self {
            clock: super::surface_poll::SurfacePollClock::new(interval),
            pending: false,
            timers: Vec::new(),
        }
    }
    pub(super) fn observe(&mut self, pending: bool) {
        self.pending = pending;
    }
    pub(super) fn observe_work(
        &mut self,
        pending: bool,
        work: impl IntoIterator<Item = pelt_core::PeltDocumentWork<ContentKey>>,
        pump_started: std::time::Instant,
    ) {
        let work: Vec<_> = work.into_iter().collect();
        self.observe(
            pending
                || work
                    .iter()
                    .any(|work| work.work.microtasks || work.work.external),
        );
        self.timers = work
            .into_iter()
            .filter_map(|observation| {
                let delay = observation.work.timer?;
                let deadline = inker::session_engine::SessionPendingWork::timer(delay)
                    .next_wake(pump_started)?;
                Some((observation, deadline))
            })
            .collect();
    }
    pub(super) fn due_work(
        &self,
        now: std::time::Instant,
    ) -> Vec<pelt_core::PeltDocumentWork<ContentKey>> {
        self.timers
            .iter()
            .filter(|(_, deadline)| *deadline <= now)
            .map(|(work, _)| work.clone())
            .collect()
    }
    pub(super) fn deadline(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        self.clock
            .deadline(self.pending, now)
            .into_iter()
            .chain(self.timers.iter().map(|(_, deadline)| *deadline))
            .min()
    }
    pub(super) fn poll_due(&mut self, now: std::time::Instant) -> bool {
        self.clock.poll_due(self.pending, now)
            || self.timers.iter().any(|(_, deadline)| *deadline <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::{Rect, SurfaceKind};
    use pelt_core::{
        PeltController, PeltControllerConfig, PeltRoute, PeltRouteSource, PeltRouteState,
    };

    fn reader<'a>(
        contents: &'a NodeDocuments,
        key: &ContentKey,
    ) -> &'a mere_document_lanes::ReaderDocumentSession {
        contents
            .pool
            .content(key)
            .unwrap()
            .document()
            .unwrap()
            .session()
            .as_any_ref()
            .downcast_ref()
            .unwrap()
    }

    #[test]
    fn settled_timer_deadline_survives_early_events_and_uses_configured_runnable_cadence() {
        let start = std::time::Instant::now();
        let work = pelt_core::PeltDocumentWork {
            content: ContentKey::Node(uuid::Uuid::from_u128(7)),
            identity: pelt_core::PeltSessionIdentity {
                instance_id: 41,
                generation: 2,
            },
            work: inker::session_engine::SessionPendingWork::timer(std::time::Duration::from_secs(
                1,
            )),
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
        let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
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
    fn physical_placements_do_not_apply_native_dpi_twice() {
        let surface = Surface {
            id: SurfaceId(3),
            kind: SurfaceKind::Content(uuid::Uuid::from_u128(3)),
            rect: Rect::new(20.0, 30.0, 640.0, 480.0),
        };
        for scale in [2.0, 0.75] {
            let placement = HostPlacement::physical(surface, 1, false, scale);
            assert_eq!(placement.surface.rect, surface.rect);
            assert_eq!(placement.scale_factor, 1.0);
        }
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
                Arc::new(super::super::standard_content_engines()),
                Arc::new(inker::SurfaceEngineRegistry::new()),
                PeltControllerConfig::from_request(
                    engine,
                    inker::SessionSpawnRequest::new(address)
                        .with_body(body)
                        .with_viewport(180, 300),
                )
                .with_host_history(),
                super::super::ShellClock(std::time::Instant::now()),
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
                super::super::standard_route_policy().route(&inker::EngineRouteRequest {
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
            let clicked = pool.dispatch_accessibility_action(
                &wide,
                &action(&wide, inker::DocumentA11yAction::Click),
            );
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
        let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
            workspace_id: inker::WorkspaceRouteId::new("terminal-pool-animation"),
            view: None,
            node: None,
            address: request.address.clone(),
            content_type: None,
            pinned_engine: Some(inker::routing::ENGINE_GENET_LIVERY.into()),
        });
        let controller = PeltController::new_shared(
            Arc::new(super::super::standard_content_engines()),
            Arc::new(inker::SurfaceEngineRegistry::new()),
            PeltControllerConfig::from_request(decision.engine_id.clone(), request)
                .with_host_history(),
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
        let engines = Arc::new(super::super::standard_content_engines());
        let surface_engines = Arc::new(inker::SurfaceEngineRegistry::new());
        let request=inker::SessionSpawnRequest::new("https://fixture.test/animation")
            .with_body("<html><head><style>@keyframes fade { from { opacity:0.2; } to { opacity:0.8; } } #a { animation:fade 1000ms linear; width:100px; height:100px; background:red; }</style></head><body><div id=a>fade</div></body></html>")
            .with_viewport(640,480);
        let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
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
            PeltControllerConfig::from_request(decision.engine_id.clone(), request)
                .with_host_history(),
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
        let engines = Arc::new(super::super::standard_content_engines());
        let surfaces = Arc::new(inker::SurfaceEngineRegistry::new());
        let epoch = std::time::Instant::now();
        let body = format!("<html><body><main><h1>Article</h1><p><a href='/next'>Reader continuation</a> offers substantial introductory article content.</p>{}</main></body></html>",
            "<p>This is substantial reader article content with enough words to retain and scroll independently.</p>".repeat(150));
        let request = inker::SessionSpawnRequest::new("https://fixture.test/story")
            .with_body(body)
            .with_content_type("text/html")
            .with_viewport(640, 480);
        let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
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
            PeltControllerConfig::from_request(decision.engine_id.clone(), request)
                .with_host_history(),
            super::super::ShellClock(epoch),
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
                        rect: Rect::new(0., 0., 320., 200.),
                    },
                    window: 0,
                    focused: true,
                    scale_factor: 1.,
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
        assert!(contents.pool.frame(&SurfaceId(11)).is_some());
        assert!(contents.pool.frame(&SurfaceId(12)).is_some());
        assert_eq!(reader(&contents, &first).viewport(), (320, 200));
        assert_eq!(reader(&contents, &second).viewport(), (640, 300));
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
            super::super::ShellClock(epoch),
        )
        .unwrap();
        let route = canonical.route().clone();
        contents.insert(node, PeltContent::from_controller(controller, route));
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
}
