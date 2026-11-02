// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Upstream Servo views on Turnstone's existing wgpu device.
//!
//! The process root lives on the UI thread, including between the last closed
//! view and a later reopen. Upstream options and JS initialization are process
//! scoped. Each view has its own rendering context and one ordered delegate
//! queue. The shell must release producers and cached frames before shutdown.

#[cfg(target_os = "macos")]
#[path = "servo_cue.rs"]
pub mod cue;
#[cfg(target_os = "linux")]
#[path = "servo_cue_linux.rs"]
pub mod cue;
#[path = "servo_cue_capabilities.rs"]
mod cue_capabilities;

use graft_engine::{GraftFrame, GraftProducerFactory, GraftSurface};
use grafting::{DiagnosticGpuSync, ImportedTexture, TextureOrigin};
use inker::{
    CursorShape, DragEvent, DragOperationSet, FocusReason, KeyboardEvent, MouseButton, MouseEvent,
    MouseEventKind, NativeTextureHandle, NavigationEvent, OwnedSurfaceFrame, PhysicalPosition,
    PointerEvent, PointerPhase, PointerType, SurfaceAccessibilityActionRequest,
    SurfaceAccessibilityTreeId, SurfaceAccessibilityUpdate, SurfaceError, SurfaceSettings,
    SurfaceSpawnRequest, SurfaceSyncHandle, SurfaceTextureFormat, WebFeatureStatus,
    WebFrameTransportMode, WebMessage, WebSurfaceCapabilities, WebSurfaceEvent,
};
use servo::{
    DevicePoint, EventLoopWaker, InputEvent, Key, KeyState, LoadStatus, Location, Modifiers,
    MouseButtonAction, MouseButtonEvent, MouseMoveEvent, NamedKey, Opts, PermissionRequest,
    Preferences, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate, WebViewPoint,
    WheelDelta, WheelEvent, WheelMode,
};
#[cfg(target_os = "linux")]
use servo_wgpu_interop_adapter::ServoDmaBufAdapter;
#[cfg(not(target_os = "linux"))]
use servo_wgpu_interop_adapter::ServoWgpuInteropAdapter;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    path::PathBuf,
    rc::Rc,
    sync::{Arc, OnceLock},
    thread::ThreadId,
};
use url::Url;
use winit::dpi::PhysicalSize;

#[cfg(not(target_os = "linux"))]
pub(crate) const FRAME_KIND: &str = "turnstone.servo.imported-frame";
#[cfg(target_os = "linux")]
pub(crate) const FRAME_KIND: &str = "turnstone.servo.native-dmabuf-frame";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ServoHostOptions {
    pub profile_name: String,
    pub profile_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum A11yFocusPolicy {
    Upstream,
    DocumentRoot,
}

static PROCESS_OWNER: OnceLock<ThreadId> = OnceLock::new();
thread_local! {
    static ROOT: RefCell<Option<Rc<ProcessRoot>>> = const { RefCell::new(None) };
    static INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

struct ProcessRoot {
    servo: Servo,
    options: ServoHostOptions,
    views: Cell<usize>,
    wake: Arc<dyn Fn() + Send + Sync>,
    a11y_focus_policy: A11yFocusPolicy,
}

#[derive(Clone)]
struct Waker(Arc<dyn Fn() + Send + Sync>);

pub(crate) struct TurnstoneServoFactory {
    owner: ThreadId,
    options: ServoHostOptions,
    device: wgpu::Device,
    queue: wgpu::Queue,
    a11y_focus_policy: A11yFocusPolicy,
}

#[derive(Debug)]
pub(crate) struct ServoImportedFrame {
    pub imported: ImportedTexture,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub resource_epoch: u64,
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct ServoDmaBufFrame {
    pub frame: wgpu_cue::webcue::OwnedNativeFrame,
}

struct Delegate {
    events: RefCell<VecDeque<WebSurfaceEvent>>,
    cursors: RefCell<VecDeque<CursorShape>>,
    accessibility_updates: RefCell<VecDeque<SurfaceAccessibilityUpdate>>,
    accessibility_active: Cell<bool>,
    accessibility_focus_provenance: Cell<u32>,
    ready: Cell<bool>,
    animating: Cell<bool>,
    requested_url: RefCell<String>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

struct TurnstoneServoSurface {
    view: Option<WebView>,
    #[cfg(not(target_os = "linux"))]
    interop: Option<ServoWgpuInteropAdapter>,
    #[cfg(target_os = "linux")]
    interop: Option<ServoDmaBufAdapter>,
    delegate: Rc<Delegate>,
    root: Rc<ProcessRoot>,
    frame_epoch: u64,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pressed_keys: HashMap<u32, Key>,
}

#[cfg(test)]
#[path = "servo/tests.rs"]
mod tests;

#[path = "servo/surface_update_from_servo.rs"]
mod surface_update_from_servo;
use surface_update_from_servo::surface_update_from_servo;
#[path = "servo/a11y_focus_policy_name.rs"]
mod a11y_focus_policy_name;
#[path = "servo/parse_a11y_focus_policy.rs"]
mod parse_a11y_focus_policy;
use parse_a11y_focus_policy::parse_a11y_focus_policy;
#[path = "servo/requested_a11y_focus_policy.rs"]
mod requested_a11y_focus_policy;
use requested_a11y_focus_policy::requested_a11y_focus_policy;
#[path = "servo/impl_event_loop_waker_for_waker.rs"]
mod impl_event_loop_waker_for_waker;
#[path = "servo/pump.rs"]
mod pump;
#[path = "servo/turnstone_servo_factory_profile_name.rs"]
mod turnstone_servo_factory_profile_name;
pub(crate) use pump::pump;
#[path = "servo/active_views.rs"]
mod active_views;
pub(crate) use active_views::active_views;
#[path = "servo/shutdown.rs"]
mod shutdown;
pub(crate) use shutdown::shutdown;
#[path = "servo/delegate_push.rs"]
mod delegate_push;
#[path = "servo/impl_drop_for_turnstone_servo_surface.rs"]
mod impl_drop_for_turnstone_servo_surface;
#[path = "servo/impl_graft_producer_factory_for_turnstone.rs"]
mod impl_graft_producer_factory_for_turnstone;
#[path = "servo/impl_graft_surface_for_turnstone_servo.rs"]
mod impl_graft_surface_for_turnstone_servo;
#[path = "servo/impl_owned_surface_frame_for_servo.rs"]
mod impl_owned_surface_frame_for_servo;
#[path = "servo/impl_owned_surface_frame_for_servo_2.rs"]
#[cfg(target_os = "linux")]
mod impl_owned_surface_frame_for_servo_2;
#[path = "servo/impl_web_view_delegate_for_delegate.rs"]
mod impl_web_view_delegate_for_delegate;
#[path = "servo/logical_key.rs"]
mod logical_key;
#[path = "servo/turnstone_servo_surface_view.rs"]
mod turnstone_servo_surface_view;
use logical_key::logical_key;
#[path = "servo/require_owner.rs"]
mod require_owner;
use require_owner::require_owner;
#[path = "servo/spawn_error.rs"]
mod spawn_error;
use spawn_error::spawn_error;
#[path = "servo/requested_gpu_sync.rs"]
mod requested_gpu_sync;
use requested_gpu_sync::requested_gpu_sync;
#[path = "servo/parse_gpu_sync.rs"]
mod parse_gpu_sync;
use parse_gpu_sync::parse_gpu_sync;
#[path = "servo/unsupported.rs"]
mod unsupported;
use unsupported::unsupported;
