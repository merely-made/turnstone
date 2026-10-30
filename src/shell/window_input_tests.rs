// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! A surface the host spawned and routed itself, held in the same content as
//! a document: one command API reaches its web plane, and the host keeps the
//! producer for the work the content does not wrap.

use super::*;
use crate::surface::{Rect, SurfaceId, SurfaceKind};
use std::sync::{Arc, Mutex};

use inker::routing::{EngineRouteDecision, SurfaceContract, SurfaceContractMode, SurfaceTargetId};
use inker::{
    Cookie, FocusReason, KeyboardEvent, MouseEvent, SessionNavigationCommand, SessionSpawnRequest,
    SurfaceError, SurfaceFrame, SurfaceProducer, SurfaceSettings, WebSurface, WebSurfaceEvent,
};
use pelt_core::{PeltContent, PeltLane, PeltRoute, PeltRouteSource, PeltRouteState};

type Calls = Arc<Mutex<Vec<String>>>;

/// A web page that records every control it receives and reports one title.
struct Page {
    calls: Calls,
    pointers: Arc<Mutex<Vec<inker::PointerEvent>>>,
    acquisitions: Arc<std::sync::atomic::AtomicUsize>,
    title: Option<String>,
    fail_offset: Option<(i32, i32)>,
}

struct OverlapDocument {
    inputs: Arc<Mutex<Vec<inker::SessionInput>>>,
    submit: bool,
}

struct CallerCaptureFixture {
    app: crate::app::App,
    contents: super::super::content_pool::NodeDocuments,
    window: u64,
    nodes: [uuid::Uuid; 2],
    surfaces: [Surface; 2],
    pointers: [Arc<Mutex<Vec<inker::PointerEvent>>>; 2],
    inputs: [Arc<Mutex<Vec<inker::SessionInput>>>; 2],
    buttons: HashMap<u64, PointerButtons>,
    /// The window's layout scale the placements are registered at.
    scale: f64,
}

/// The primary native caller below its chrome/modal/context admission gates.
/// It uses the actual App plan, NodeDocuments and Pelt pool. Only renderer and
/// OS cursor/redraw effects are absent. In particular this fixture does not
/// call content_press_target directly or assume that every hit is Content.
struct PrimaryPaneCaptureFixture {
    content: CallerCaptureFixture,
    capture: Option<SurfaceKind>,
    delivered: Vec<(ContentKey, bool)>,
    cancellation_capture: Vec<Option<SurfaceKind>>,
}

/// An independent receiver with observable focus and Drop, used only by the
/// interception controls. The original native fixtures and tests stay intact.
struct InterceptionPage {
    page: Page,
    lifetime: Calls,
}

/// Lens caller below native bookkeeping, with a recording retained pane
/// receiver. App plans, content pool and both production admission/terminal
/// seams are real; only the contributed renderer and native Window are absent.
struct LensPaneCustodyFixture {
    content: CallerCaptureFixture,
    admitted: Option<super::super::input::ContributedPointer>,
    capture: Option<SurfaceKind>,
    pane_events: Vec<(PointerPhase, u64, (f32, f32))>,
    generation: u64,
}

#[path = "window_input_tests/document_text_input_geometry_maps_current_shared_caret_cases.rs"]
mod document_text_input_geometry_maps_current_shared_caret_cases;
#[path = "window_input_tests/review_input_touch_second_appearance_coordinates_and_sensors_cases.rs"]
mod review_input_touch_second_appearance_coordinates_and_sensors_cases;
#[path = "window_input_tests/review_native_interception_lens_admitted_chord_precedes_fresh_cases.rs"]
mod review_native_interception_lens_admitted_chord_precedes_fresh_cases;
#[path = "window_input_tests/review_overlap_native_reader_appearances_wheel_mutates_only_cases.rs"]
mod review_overlap_native_reader_appearances_wheel_mutates_only_cases;

#[path = "window_input_tests/impl_surface_producer_for_page.rs"]
mod impl_surface_producer_for_page;
#[path = "window_input_tests/impl_web_surface_for_page.rs"]
mod impl_web_surface_for_page;
#[path = "window_input_tests/page_record.rs"]
mod page_record;
#[path = "window_input_tests/route.rs"]
mod route;
use route::route;
#[path = "window_input_tests/hosted.rs"]
mod hosted;
use hosted::hosted;
#[path = "window_input_tests/key.rs"]
mod key;
use key::key;
#[path = "window_input_tests/appearance.rs"]
mod appearance;
use appearance::appearance;
#[path = "window_input_tests/placed.rs"]
mod placed;
use placed::placed;
#[path = "window_input_tests/touch_pool.rs"]
mod touch_pool;
use touch_pool::touch_pool;
#[path = "window_input_tests/touch.rs"]
mod touch;
use touch::touch;
#[path = "window_input_tests/window_keyboard_case.rs"]
mod window_keyboard_case;
use window_keyboard_case::window_keyboard_case;
#[path = "window_input_tests/keyboard.rs"]
mod keyboard;
use keyboard::keyboard;
#[path = "window_input_tests/native_mouse.rs"]
mod native_mouse;
use native_mouse::native_mouse;
#[path = "window_input_tests/cancelled_native_mouse_case.rs"]
mod cancelled_native_mouse_case;
use cancelled_native_mouse_case::cancelled_native_mouse_case;
#[path = "window_input_tests/bookkeeping_with_existing_mouse_capture.rs"]
mod bookkeeping_with_existing_mouse_capture;
use bookkeeping_with_existing_mouse_capture::bookkeeping_with_existing_mouse_capture;
#[path = "window_input_tests/overlap_surface_pool.rs"]
mod overlap_surface_pool;
use overlap_surface_pool::overlap_surface_pool;
#[path = "window_input_tests/impl_inker_document_session_netrender_scene.rs"]
mod impl_inker_document_session_netrender_scene;
#[path = "window_input_tests/overlap_native_document_effect.rs"]
mod overlap_native_document_effect;
use overlap_native_document_effect::overlap_native_document_effect;
#[path = "window_input_tests/caller_capture_fixture_new.rs"]
mod caller_capture_fixture_new;
#[path = "window_input_tests/caller_capture_gesture.rs"]
mod caller_capture_gesture;
use caller_capture_gesture::caller_capture_gesture;
#[path = "window_input_tests/caller_capture_effect.rs"]
mod caller_capture_effect;
use caller_capture_effect::caller_capture_effect;
#[path = "window_input_tests/caller_capture_retirement.rs"]
mod caller_capture_retirement;
use caller_capture_retirement::caller_capture_retirement;
#[path = "window_input_tests/impl_super_super_input_primary_pointer_host.rs"]
mod impl_super_super_input_primary_pointer_host;
#[path = "window_input_tests/primary_pane_capture_fixture_new.rs"]
mod primary_pane_capture_fixture_new;
#[path = "window_input_tests/primary_pane_chord.rs"]
mod primary_pane_chord;
use primary_pane_chord::primary_pane_chord;
#[path = "window_input_tests/primary_no_hit_chord.rs"]
mod primary_no_hit_chord;
use primary_no_hit_chord::primary_no_hit_chord;
#[path = "window_input_tests/impl_drop_for_interception_page.rs"]
mod impl_drop_for_interception_page;
#[path = "window_input_tests/impl_surface_producer_for_interception_page.rs"]
mod impl_surface_producer_for_interception_page;
#[path = "window_input_tests/interception_page.rs"]
mod interception_page;
#[path = "window_input_tests/primary_pane_capture_fixture_open_primary_decision.rs"]
mod primary_pane_capture_fixture_open_primary_decision;
use interception_page::interception_page;
#[path = "window_input_tests/impl_super_super_input_primary_press_admission.rs"]
mod impl_super_super_input_primary_press_admission;
#[path = "window_input_tests/interception_fixture.rs"]
mod interception_fixture;
use interception_fixture::interception_fixture;
#[path = "window_input_tests/assert_interception_retired.rs"]
mod assert_interception_retired;
use assert_interception_retired::assert_interception_retired;
#[path = "window_input_tests/interception_bookkeeping.rs"]
mod interception_bookkeeping;
use interception_bookkeeping::interception_bookkeeping;
#[path = "window_input_tests/lens_fresh_pane.rs"]
mod lens_fresh_pane;
use lens_fresh_pane::lens_fresh_pane;
#[path = "window_input_tests/lens_interception_chord.rs"]
mod lens_interception_chord;
use lens_interception_chord::lens_interception_chord;
#[path = "window_input_tests/impl_super_super_lens_lens_pane_pointer.rs"]
mod impl_super_super_lens_lens_pane_pointer;
#[path = "window_input_tests/lens_pane_custody_fixture_new.rs"]
mod lens_pane_custody_fixture_new;
#[path = "window_input_tests/modal_touch_open.rs"]
mod modal_touch_open;
use modal_touch_open::modal_touch_open;
#[path = "window_input_tests/modal_touch_close.rs"]
mod modal_touch_close;
use modal_touch_close::modal_touch_close;
#[path = "window_input_tests/modal_touch_delivery.rs"]
mod modal_touch_delivery;
use modal_touch_delivery::modal_touch_delivery;
#[path = "window_input_tests/modal_touch_terminal.rs"]
mod modal_touch_terminal;
use modal_touch_terminal::modal_touch_terminal;
