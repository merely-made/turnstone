// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Windows system-webview construction on Turnstone's existing device.
//!
//! One capture-only composition root is shared by this factory's producers.
//! Its private HWND never overlays the product window. Each producer needs
//! its own DX12 fence: Scry 0.7.1 allocates fence values independently on each
//! D3D11 capture context, so sharing a fence would let one page satisfy another
//! page's wait. The host must import using the matching profile synchronizer.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use inker::{
    Cookie, CursorShape, FocusReason, KeyboardEvent, MouseButton, MouseEvent, PointerButtons,
    PointerEvent, PointerPhase, PointerType, SurfaceEngine, SurfaceError, SurfaceFrame,
    SurfaceProducer, SurfaceSettings, SurfaceSpawnRequest, WebFeatureStatus, WebRequestId,
    WebSurface, WebSurfaceCapabilities, WebSurfaceEvent,
};
use scrying_engine::scrying as native;
use scrying_engine::scrying::{
    Dx12FenceSynchronizer, HostWgpuContext, PlatformCompositionRoot, PlatformWebSurfaceConfig,
    PlatformWebSurfaceProducer, WebSurfaceProducer,
};
use scrying_engine::{ProducerFactory, ScryingProducer};
use webview2_com::{CallDevToolsProtocolMethodCompletedHandler, CoTaskMemPWSTR};
use winit::dpi::PhysicalSize;

static NEXT_FACTORY_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    // COM/WinComp resources stay on the factory's construction thread. Weak
    // entries release the owned HWND when its last producer goes away; factory
    // identities keep unrelated shells from sharing a native tree.
    static COMPOSITION_ROOTS: RefCell<HashMap<u64, Weak<PlatformCompositionRoot>>> =
        RefCell::new(HashMap::new());
}

pub(super) struct TurnstoneScryFactory {
    id: u64,
    owner: ThreadId,
    host: HostWgpuContext,
    fences: Mutex<HashMap<PathBuf, Weak<Dx12FenceSynchronizer>>>,
}

/// Keep the current Inker adapter's command/event projection, while refusing
/// capabilities which that pinned adapter does not actually forward.
pub(super) struct TurnstoneScryEngine {
    factory: Arc<TurnstoneScryFactory>,
}

struct TurnstoneScryProducer {
    inner: ScryingProducer,
    producer: Rc<RefCell<PlatformWebSurfaceProducer>>,
    // Keep the outer SurfaceFrame's borrowed fence handle valid as well as
    // the native producer's own retain. Importers retain their own clone.
    _fence: Arc<Dx12FenceSynchronizer>,
}

/// Share only the owner-thread native producer. Keep the pinned adapter's
/// forwarding, with a completion-driven keyboard queue for host event loops.
/// Other native APIs retain trait defaults.
struct SharedNativeProducer(
    Rc<RefCell<PlatformWebSurfaceProducer>>,
    Rc<RefCell<KeyboardDispatchQueue>>,
);

const KEYBOARD_COMPLETION_TIMEOUT: Duration = Duration::from_secs(2);

/// Completion callbacks only update this state. The owner-thread host poll
/// submits the next command, without nesting a message loop or a COM call in
/// the callback. Waiting for completion preserves rawKeyDown/char/keyUp order.
#[derive(Default)]
struct KeyboardDispatchQueue {
    pending: VecDeque<String>,
    in_flight: Option<Instant>,
    failure: Option<String>,
}

macro_rules! forward_native {
    ($name:ident($($arg:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&mut self, $($arg: $ty),*) -> $result {
            WebSurfaceProducer::$name(&mut *self.0.borrow_mut(), $($arg),*)
        }
    };
}

#[cfg(test)]
#[path = "scry/tests.rs"]
mod tests;

#[path = "scry/impl_producer_factory_for_turnstone_scry.rs"]
mod impl_producer_factory_for_turnstone_scry;
#[path = "scry/require_owner_thread.rs"]
mod require_owner_thread;
#[path = "scry/turnstone_scry_factory_build_native.rs"]
mod turnstone_scry_factory_build_native;
#[path = "scry/turnstone_scry_factory_new.rs"]
mod turnstone_scry_factory_new;
use require_owner_thread::require_owner_thread;
#[path = "scry/profile_path.rs"]
mod profile_path;
use profile_path::profile_path;
#[path = "scry/spawn_error.rs"]
mod spawn_error;
use spawn_error::spawn_error;
#[path = "scry/impl_surface_engine_for_turnstone_scry.rs"]
mod impl_surface_engine_for_turnstone_scry;
#[path = "scry/impl_surface_producer_for_turnstone_scry.rs"]
mod impl_surface_producer_for_turnstone_scry;
#[path = "scry/mouse_pointer_input.rs"]
mod mouse_pointer_input;
#[path = "scry/turnstone_scry_engine_new.rs"]
mod turnstone_scry_engine_new;
use mouse_pointer_input::mouse_pointer_input;
#[path = "scry/keyboard_completion.rs"]
mod keyboard_completion;
#[path = "scry/keyboard_dispatch_queue_enqueue.rs"]
mod keyboard_dispatch_queue_enqueue;
#[path = "scry/shared_native_producer_new.rs"]
mod shared_native_producer_new;
use keyboard_completion::keyboard_completion;
#[path = "scry/keyboard_commands.rs"]
mod keyboard_commands;
use keyboard_commands::keyboard_commands;
#[path = "scry/keyboard_params.rs"]
mod keyboard_params;
use keyboard_params::keyboard_params;
#[path = "scry/host_capabilities.rs"]
mod host_capabilities;
#[path = "scry/impl_web_surface_for_turnstone_scry.rs"]
mod impl_web_surface_for_turnstone_scry;
#[path = "scry/impl_web_surface_producer_for_shared.rs"]
mod impl_web_surface_producer_for_shared;
use host_capabilities::host_capabilities;
#[path = "scry/projected_event_capability.rs"]
mod projected_event_capability;
use projected_event_capability::projected_event_capability;
