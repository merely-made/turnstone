// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Placement visibility and controller-owned document pumping.

use pelt_core::{PeltContent, PeltLane};
use std::{collections::HashMap, hash::Hash};

/// Notify each retained document of placement visibility, then advance their
/// clocks even while hidden. Only visible work asks for another frame. Iterating the content map pumps a document once even
/// when several windows place it. Surface producers have their own poll policy.
pub(crate) fn pump_documents<K: Eq + Hash, F: 'static>(
    contents: &mut HashMap<K, PeltContent<F>>,
    visible: &[K],
) -> bool {
    let mut redraw = false;
    for (node, content) in contents {
        if content.lane() != PeltLane::Document {
            continue;
        }
        let shown = visible.contains(node);
        content.set_hidden(!shown);
        let unsettled = content.pump();
        redraw |= shown && unsettled;
    }
    redraw
}

#[cfg(test)]
mod tests {
    use super::*;
    use inker::routing::{
        EngineRouteDecision, SurfaceContract, SurfaceContractMode, SurfaceTargetId,
    };
    use inker::{
        DocumentSession, SessionClick, SessionEngine, SessionError, SessionRegistry,
        SessionScrollKey, SessionSpawnRequest, SurfaceEngineRegistry,
    };
    use pelt_core::{
        PeltClock, PeltContent, PeltController, PeltControllerConfig, PeltRoute, PeltRouteSource,
        PeltRouteState,
    };
    use std::{
        any::Any,
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    #[derive(Default)]
    struct Calls {
        hidden: Vec<bool>,
        pumps: Vec<f64>,
        scroll: f32,
        settled: bool,
        settle_on_pump: bool,
    }
    struct Engine(Arc<Mutex<Calls>>);
    struct Page(Arc<Mutex<Calls>>);
    impl SessionEngine<String> for Engine {
        fn engine_id(&self) -> &str {
            "fake"
        }
        fn spawn(
            &self,
            _: &SessionSpawnRequest,
        ) -> Result<Box<dyn DocumentSession<String>>, SessionError> {
            Ok(Box::new(Page(self.0.clone())))
        }
    }
    impl DocumentSession<String> for Page {
        fn frame(&mut self, _: u32, _: u32) -> String {
            String::new()
        }
        fn scroll_by(&mut self, _: f32, dy: f32) -> bool {
            self.0.lock().unwrap().scroll += dy;
            true
        }
        fn scroll_for_key(&mut self, _: SessionScrollKey) -> bool {
            false
        }
        fn click_at(&mut self, _: f32, _: f32) -> SessionClick {
            SessionClick::Miss
        }
        fn links(&self) -> Vec<inker::SessionLink> {
            Vec::new()
        }
        fn pump(&mut self, now: f64) {
            let mut calls = self.0.lock().unwrap();
            calls.pumps.push(now);
            if calls.settle_on_pump {
                calls.settled = true;
            }
        }
        fn settled(&mut self) -> bool {
            self.0.lock().unwrap().settled
        }
        fn set_hidden(&mut self, hidden: bool) {
            self.0.lock().unwrap().hidden.push(hidden);
        }
        fn as_any_ref(&self) -> &dyn Any {
            self
        }
        fn as_any(&mut self) -> &mut dyn Any {
            self
        }
    }
    struct Clock;
    impl PeltClock for Clock {
        fn now_ms(&self) -> f64 {
            987.5
        }
    }
    fn document(calls: &Arc<Mutex<Calls>>) -> PeltContent<String> {
        let mut sessions = SessionRegistry::new();
        sessions.register(Box::new(Engine(calls.clone())));
        let controller = PeltController::new(
            sessions,
            SurfaceEngineRegistry::new(),
            PeltControllerConfig::new("fake", "gemini://test/", (640, 480)).with_host_history(),
            Clock,
        )
        .unwrap();
        PeltContent::from_controller(
            controller,
            PeltRoute {
                decision: EngineRouteDecision {
                    engine_id: "fake".into(),
                    surface_contract: SurfaceContract {
                        target: SurfaceTargetId::new("test"),
                        mode: SurfaceContractMode::CompositedTexture,
                    },
                },
                source: PeltRouteSource::Automatic,
                state: PeltRouteState::Document,
            },
        )
    }
    #[test]
    fn a_hidden_unsettled_document_keeps_its_clock_without_forcing_redraw() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let mut contents = HashMap::from([(1u8, document(&calls))]);
        assert!(!pump_documents(&mut contents, &[]));
        let calls = calls.lock().unwrap();
        assert_eq!(calls.hidden, [true]);
        assert_eq!(calls.pumps, [987.5]);
    }
    #[test]
    fn a_lens_only_document_is_visible_and_keeps_its_clock_live() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let mut contents = HashMap::from([(1u8, document(&calls))]);
        let primary: Vec<u8> = Vec::new();
        let lens = vec![1];
        let visible: Vec<_> = primary.into_iter().chain(lens).collect();
        assert!(pump_documents(&mut contents, &visible));
        let calls = calls.lock().unwrap();
        assert_eq!(calls.hidden, [false]);
        assert_eq!(calls.pumps, [987.5]);
    }
    #[test]
    fn repeated_placements_pump_one_retained_controller() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let mut contents = HashMap::from([(1u8, document(&calls))]);
        let id = contents[&1].session_identity();
        assert!(pump_documents(&mut contents, &[1, 1, 1]));
        assert_eq!(calls.lock().unwrap().pumps, [987.5]);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[&1].session_identity(), id);
    }
    #[test]
    fn hiding_and_showing_retains_identity_and_scroll_state() {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let mut contents = HashMap::from([(1u8, document(&calls))]);
        let id = contents[&1].session_identity();
        assert!(
            contents
                .get_mut(&1)
                .unwrap()
                .document_mut()
                .unwrap()
                .scroll_by(0.0, 42.0)
        );
        assert!(pump_documents(&mut contents, &[1]));
        assert!(!pump_documents(&mut contents, &[]));
        assert!(pump_documents(&mut contents, &[1]));
        assert_eq!(contents[&1].session_identity(), id);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.scroll, 42.0);
        assert_eq!(calls.hidden, [false, true, false]);
        assert_eq!(calls.pumps, [987.5, 987.5, 987.5]);
    }
    #[test]
    fn settled_documents_do_not_keep_redraw_alive() {
        let calls = Arc::new(Mutex::new(Calls {
            settled: true,
            ..Calls::default()
        }));
        let mut contents = HashMap::from([(1u8, document(&calls))]);
        assert!(!pump_documents(&mut contents, &[1]));
        assert_eq!(calls.lock().unwrap().pumps, [987.5]);
    }
    #[test]
    fn hidden_pending_work_can_reach_quiescence_without_a_redraw() {
        let calls = Arc::new(Mutex::new(Calls {
            settle_on_pump: true,
            ..Calls::default()
        }));
        let mut contents = HashMap::from([(1u8, document(&calls))]);
        assert!(!calls.lock().unwrap().settled);
        assert!(!pump_documents(&mut contents, &[]));
        assert!(
            calls.lock().unwrap().settled,
            "hidden pending work must be able to finish"
        );
    }

    struct Surface;
    impl inker::SurfaceProducer for Surface {
        fn resize(&mut self, _: u32, _: u32) -> Result<(), inker::SurfaceError> {
            Ok(())
        }
        fn set_offset(&mut self, _: i32, _: i32) -> Result<(), inker::SurfaceError> {
            Ok(())
        }
        fn acquire_frame(&mut self) -> Result<Option<inker::SurfaceFrame>, inker::SurfaceError> {
            Ok(None)
        }
        fn send_mouse_input(&mut self, _: inker::MouseEvent) -> Result<(), inker::SurfaceError> {
            Ok(())
        }
        fn send_pointer_input(
            &mut self,
            _: inker::PointerEvent,
        ) -> Result<(), inker::SurfaceError> {
            Ok(())
        }
        fn send_keyboard_input(
            &mut self,
            _: inker::KeyboardEvent,
        ) -> Result<(), inker::SurfaceError> {
            Ok(())
        }
        fn move_focus(&mut self, _: inker::FocusReason) -> Result<(), inker::SurfaceError> {
            Ok(())
        }
        fn poll_cursor_shape(&mut self) -> Option<inker::CursorShape> {
            None
        }
        fn apply_settings(
            &mut self,
            _: &inker::SurfaceSettings,
        ) -> Result<(), inker::SurfaceError> {
            Ok(())
        }
    }
    #[test]
    fn surface_polling_is_not_driven_by_the_document_pump() {
        let route = PeltRoute {
            decision: EngineRouteDecision {
                engine_id: "fake.surface".into(),
                surface_contract: SurfaceContract {
                    target: SurfaceTargetId::new("surface"),
                    mode: SurfaceContractMode::CompositedTexture,
                },
            },
            source: PeltRouteSource::Automatic,
            state: PeltRouteState::Surface,
        };
        let mut contents = HashMap::from([(
            1u8,
            PeltContent::<String>::from_surface(Box::new(Surface), (640, 480), route),
        )]);
        assert!(!pump_documents(&mut contents, &[1]));
        assert!(!pump_documents(&mut contents, &[]));
    }
}
