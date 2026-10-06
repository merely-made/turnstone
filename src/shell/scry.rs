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

impl TurnstoneScryFactory {
    /// Construct on the UI thread that will spawn and drive all producers.
    /// Device/backend and WebView2 runtime failures are returned by `build`.
    pub(super) fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            id: NEXT_FACTORY_ID.fetch_add(1, Ordering::Relaxed),
            owner: std::thread::current().id(),
            host: HostWgpuContext::new(device, queue),
            fences: Mutex::new(HashMap::new()),
        }
    }

    /// Fetch immediately after successful spawn, before accepting its frames.
    /// Each node has a distinct profile, and the shell owns at most one live
    /// producer per node. Retain the returned Arc in that node's importer for
    /// as long as any imported frame may be sampled, including after close.
    pub(super) fn synchronizer_for_profile(
        &self,
        profile: &Path,
    ) -> Result<Arc<Dx12FenceSynchronizer>, String> {
        self.fences
            .lock()
            .map_err(|_| "Scry producer-fence lookup was poisoned".to_owned())?
            .get(profile)
            .and_then(Weak::upgrade)
            .ok_or_else(|| "Scry has no live producer fence for this profile".to_owned())
    }

    fn composition_root(&self) -> Result<Arc<PlatformCompositionRoot>, SurfaceError> {
        require_owner_thread(self.owner)?;
        COMPOSITION_ROOTS.with(|roots| {
            let mut roots = roots.borrow_mut();
            roots.retain(|_, root| root.strong_count() != 0);
            if let Some(root) = roots.get(&self.id).and_then(Weak::upgrade) {
                return Ok(root);
            }
            // Capture is CreateFromVisual on each pane, independent of this
            // host window's size. The root owns its HWND for every attachment.
            let root = PlatformCompositionRoot::new_offscreen(PhysicalSize::new(1, 1))
                .map_err(|error| spawn_error("create capture-only composition root", error))?;
            roots.insert(self.id, Arc::downgrade(&root));
            Ok(root)
        })
    }
}

impl TurnstoneScryFactory {
    fn build_native(
        &self,
        request: &SurfaceSpawnRequest,
    ) -> Result<PlatformWebSurfaceProducer, SurfaceError> {
        require_owner_thread(self.owner)?;
        let profile = profile_path(&request.profile.user_data_dir)?;
        if request.fence_handle.is_some() {
            return Err(SurfaceError::SpawnFailed(
                "Scry capture uses a dedicated producer fence; a raw spawn fence cannot replace its synchronizer".into(),
            ));
        }
        let fence = Arc::new(
            Dx12FenceSynchronizer::new(&self.host)
                .map_err(|error| spawn_error("create producer DX12 fence", error))?,
        );
        std::fs::create_dir_all(&profile).map_err(|error| {
            SurfaceError::SpawnFailed(format!(
                "could not create Scry profile {}: {error}",
                profile.display()
            ))
        })?;
        let root = self.composition_root()?;
        let config = PlatformWebSurfaceConfig::new(
            PhysicalSize::new(request.width.max(1), request.height.max(1)),
            &profile,
        )
        .with_dx12_fence_synchronizer(fence.clone());
        // SAFETY: root owns the live offscreen HWND, every attached producer
        // retains it, and this factory rejects construction off its UI thread.
        let producer = unsafe { PlatformWebSurfaceProducer::new_attached(&root, config) }
            .map_err(|error| spawn_error("construct WebView2 capture producer", error))?;
        // Start the requested URL promptly. Completion belongs to the ordered
        // event stream, rather than a blocking navigation wait in spawn.
        producer
            .load_url(&request.url)
            .map_err(|error| spawn_error("start initial navigation", error))?;
        let mut fences = self.fences.lock().map_err(|_| {
            SurfaceError::SpawnFailed("Scry producer-fence lookup was poisoned".into())
        })?;
        fences.retain(|_, fence| fence.strong_count() != 0);
        fences.insert(profile, Arc::downgrade(&fence));
        Ok(producer)
    }
}

impl ProducerFactory for TurnstoneScryFactory {
    fn build(
        &self,
        request: &SurfaceSpawnRequest,
    ) -> Result<Box<dyn WebSurfaceProducer>, SurfaceError> {
        Ok(Box::new(self.build_native(request)?))
    }
}

fn require_owner_thread(owner: ThreadId) -> Result<(), SurfaceError> {
    if owner != std::thread::current().id() {
        return Err(SurfaceError::SpawnFailed(
            "Scry producers must be constructed on their factory's UI thread".into(),
        ));
    }
    Ok(())
}

fn profile_path(profile: &str) -> Result<PathBuf, SurfaceError> {
    let path = PathBuf::from(profile);
    if profile.trim().is_empty() || !path.is_absolute() {
        return Err(SurfaceError::SpawnFailed(
            "Scry requires an explicit absolute per-node profile directory".into(),
        ));
    }
    Ok(path)
}

fn spawn_error(stage: &str, error: impl std::fmt::Display) -> SurfaceError {
    SurfaceError::SpawnFailed(format!("Scry could not {stage}: {error}"))
}

/// Keep the current Inker adapter's command/event projection, while refusing
/// capabilities which that pinned adapter does not actually forward.
pub(super) struct TurnstoneScryEngine {
    factory: Arc<TurnstoneScryFactory>,
}

impl TurnstoneScryEngine {
    pub(super) fn new(factory: Arc<TurnstoneScryFactory>) -> Self {
        Self { factory }
    }
}

impl SurfaceEngine for TurnstoneScryEngine {
    fn engine_id(&self) -> &str {
        inker::routing::ENGINE_SCRYING_WEB
    }

    fn spawn(
        &self,
        request: &SurfaceSpawnRequest,
    ) -> Result<Box<dyn SurfaceProducer>, SurfaceError> {
        let producer = Rc::new(RefCell::new(self.factory.build_native(request)?));
        let fence = self
            .factory
            .synchronizer_for_profile(Path::new(&request.profile.user_data_dir))
            .map_err(|error| spawn_error("retain producer fence", error))?;
        let handle = fence.shared_handle().0 as usize as u64;
        Ok(Box::new(TurnstoneScryProducer {
            inner: ScryingProducer::new(
                Box::new(SharedNativeProducer::new(producer.clone())),
                Some(handle),
            ),
            producer,
            _fence: fence,
        }))
    }
}

struct TurnstoneScryProducer {
    inner: ScryingProducer,
    producer: Rc<RefCell<PlatformWebSurfaceProducer>>,
    // Keep the outer SurfaceFrame's borrowed fence handle valid as well as
    // the native producer's own retain. Importers retain their own clone.
    _fence: Arc<Dx12FenceSynchronizer>,
}

impl SurfaceProducer for TurnstoneScryProducer {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.inner.resize(width, height)
    }

    fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        self.inner.set_offset(x, y)
    }

    fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
        self.inner.acquire_frame()
    }

    fn send_mouse_input(&mut self, ev: MouseEvent) -> Result<(), SurfaceError> {
        self.inner.send_mouse_input(ev)
    }

    fn send_pointer_input(&mut self, ev: PointerEvent) -> Result<(), SurfaceError> {
        if ev.pointer_type == PointerType::Mouse {
            // WebView2 SendPointerInput accepts touch and pen only. Bypass
            // Inker's older MouseEvent shape, which lacks modifiers and Leave.
            let input = mouse_pointer_input(&ev)?;
            tracing::debug!(target: "turnstone::scry_input", ?input, "begin native mouse delivery");
            let result = self.producer
                .borrow()
                .send_mouse_input(input)
                .map_err(scrying_engine::translation::map_error);
            tracing::debug!(target: "turnstone::scry_input", ok = result.is_ok(), "finished native mouse delivery");
            result
        } else {
            self.inner.send_pointer_input(ev)
        }
    }

    fn send_keyboard_input(&mut self, ev: KeyboardEvent) -> Result<(), SurfaceError> {
        self.inner.send_keyboard_input(ev)
    }

    fn move_focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        tracing::debug!(target: "turnstone::scry_input", ?reason, "begin native focus delivery");
        let result = self.inner.move_focus(reason);
        tracing::debug!(target: "turnstone::scry_input", ok = result.is_ok(), "finished native focus delivery");
        result
    }

    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.inner.poll_cursor_shape()
    }

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        self.inner.apply_settings(settings)
    }

    // Rehosting would move the private capture HWND into a product window or
    // fail while the root is shared. Retain the trait's explicit refusal.
    fn as_web_surface(&mut self) -> Option<&mut dyn WebSurface> {
        Some(self)
    }
}

fn mouse_pointer_input(ev: &PointerEvent) -> Result<native::MouseInput, SurfaceError> {
    use native::MouseEventKind as Kind;
    let (kind, mouse_data) = match (ev.phase, ev.button) {
        (PointerPhase::Move, _) => (Kind::Move, 0),
        (PointerPhase::Cancel, _) => (Kind::Leave, 0),
        (PointerPhase::Down, Some(MouseButton::Left)) => (Kind::LeftButtonDown, 0),
        (PointerPhase::Up, Some(MouseButton::Left)) => (Kind::LeftButtonUp, 0),
        (PointerPhase::Down, Some(MouseButton::Middle)) => (Kind::MiddleButtonDown, 0),
        (PointerPhase::Up, Some(MouseButton::Middle)) => (Kind::MiddleButtonUp, 0),
        (PointerPhase::Down, Some(MouseButton::Right)) => (Kind::RightButtonDown, 0),
        (PointerPhase::Up, Some(MouseButton::Right)) => (Kind::RightButtonUp, 0),
        (PointerPhase::Down, Some(MouseButton::Back)) => (Kind::XButtonDown, 1),
        (PointerPhase::Up, Some(MouseButton::Back)) => (Kind::XButtonUp, 1),
        (PointerPhase::Down, Some(MouseButton::Forward)) => (Kind::XButtonDown, 2),
        (PointerPhase::Up, Some(MouseButton::Forward)) => (Kind::XButtonUp, 2),
        (_, None) => {
            return Err(SurfaceError::InputFailed(
                "Scry mouse Down/Up requires the changed button".into(),
            ));
        },
    };
    Ok(native::MouseInput {
        kind,
        virtual_keys: native::MouseVirtualKeys {
            control: ev.modifiers.ctrl,
            shift: ev.modifiers.shift,
            left_button: ev.buttons.contains(PointerButtons::PRIMARY),
            right_button: ev.buttons.contains(PointerButtons::SECONDARY),
            middle_button: ev.buttons.contains(PointerButtons::AUXILIARY),
            x_button1: ev.buttons.contains(PointerButtons::BACK),
            x_button2: ev.buttons.contains(PointerButtons::FORWARD),
        },
        mouse_data,
        point: (ev.position.x as i32, ev.position.y as i32),
    })
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

impl KeyboardDispatchQueue {
    fn enqueue(&mut self, commands: Vec<String>) -> Result<(), native::WebSurfaceError> {
        self.check_failure()?;
        self.pending.extend(commands);
        Ok(())
    }

    fn check_failure(&self) -> Result<(), native::WebSurfaceError> {
        match &self.failure {
            Some(reason) => Err(native::WebSurfaceError::Platform(reason.clone())),
            None => Ok(()),
        }
    }

    fn next(&mut self, now: Instant) -> Result<Option<String>, native::WebSurfaceError> {
        self.check_failure()?;
        if let Some(started) = self.in_flight {
            if now.saturating_duration_since(started) >= KEYBOARD_COMPLETION_TIMEOUT {
                self.complete(Err(
                    "Scry keyboard CDP completion timed out without pumping the UI".into(),
                ));
                self.check_failure()?;
            }
            return Ok(None);
        }
        let command = self.pending.pop_front();
        if command.is_some() {
            self.in_flight = Some(now);
        }
        Ok(command)
    }

    fn complete(&mut self, result: Result<(), String>) {
        // A timed-out or retired request cannot revive a failed stream.
        if self.in_flight.take().is_none() || self.failure.is_some() {
            return;
        }
        if let Err(reason) = result {
            self.failure = Some(reason);
            self.pending.clear();
        }
    }
}

impl SharedNativeProducer {
    fn new(producer: Rc<RefCell<PlatformWebSurfaceProducer>>) -> Self {
        Self(
            producer,
            Rc::new(RefCell::new(KeyboardDispatchQueue::default())),
        )
    }

    fn drive_keyboard(&self) -> Result<(), native::WebSurfaceError> {
        let Some(params) = self.1.borrow_mut().next(Instant::now())? else {
            return Ok(());
        };
        let weak_queue = Rc::downgrade(&self.1);
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |result: windows::core::Result<()>, json_result: String| {
                if let Some(queue) = weak_queue.upgrade() {
                    queue.borrow_mut().complete(keyboard_completion(
                        result.map_err(|error| error.to_string()),
                        &json_result,
                    ));
                }
                Ok(())
            },
        ));
        let method = CoTaskMemPWSTR::from("Input.dispatchKeyEvent");
        let params = CoTaskMemPWSTR::from(params.as_str());
        // SAFETY: the producer and callback stay on their construction thread;
        // WebView2 retains the callback for this asynchronous operation. The
        // callback holds only Weak queue state and cannot retain a closed page.
        let result = unsafe {
            tracing::debug!(target: "turnstone::scry_input", "begin keyboard CDP submission");
            self.0.borrow().webview().CallDevToolsProtocolMethod(
                *method.as_ref().as_pcwstr(),
                *params.as_ref().as_pcwstr(),
                &handler,
            )
        };
        tracing::debug!(target: "turnstone::scry_input", ok = result.is_ok(), "finished keyboard CDP submission");
        if let Err(error) = result {
            self.1
                .borrow_mut()
                .complete(Err(format!("Scry keyboard CDP dispatch failed: {error}")));
        }
        self.1.borrow().check_failure()
    }
}

fn keyboard_completion(result: Result<(), String>, json: &str) -> Result<(), String> {
    result.map_err(|error| format!("Scry keyboard CDP completion failed: {error}"))?;
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| format!("Scry keyboard CDP completion was invalid JSON: {error}"))?;
    if let Some(error) = value.get("error") {
        return Err(format!("Scry keyboard CDP rejected input: {error}"));
    }
    Ok(())
}

fn keyboard_commands(
    event: &native::KeyboardInput,
) -> Result<Vec<String>, native::WebSurfaceError> {
    use native::KeyEventKind;
    let commands = match event.kind {
        KeyEventKind::Down => {
            let mut commands = vec![keyboard_params("rawKeyDown", event, None)];
            if !event.characters.is_empty() {
                commands.push(keyboard_params("char", event, Some(&event.characters)));
            }
            commands
        },
        KeyEventKind::Up => vec![keyboard_params("keyUp", event, None)],
        KeyEventKind::ModifiersChanged => {
            let down = match event.virtual_key_code {
                0x10 | 0xA0 | 0xA1 => event.modifiers.shift,
                0x11 | 0xA2 | 0xA3 => event.modifiers.control,
                0x12 | 0xA4 | 0xA5 => event.modifiers.alt,
                0x5B | 0x5C => event.modifiers.meta,
                0x14 => event.modifiers.caps_lock,
                _ => false,
            };
            vec![keyboard_params(
                if down { "rawKeyDown" } else { "keyUp" },
                event,
                None,
            )]
        },
        _ => {
            return Err(native::WebSurfaceError::Unsupported(
                "Turnstone has no CDP mapping for this keyboard event kind",
            ));
        },
    };
    Ok(commands)
}

fn keyboard_params(kind: &str, event: &native::KeyboardInput, text: Option<&str>) -> String {
    // Match Scry 0.7.1's key identities and native virtual-key fields. JSON
    // serialization preserves Unicode, quotes and control characters.
    let (key, code) = match event.virtual_key_code {
        0x30..=0x39 => {
            let ch = char::from_u32(event.virtual_key_code).unwrap_or('0');
            (ch.to_string(), format!("Digit{ch}"))
        },
        0x41..=0x5A => {
            let upper = char::from_u32(event.virtual_key_code).unwrap_or('A');
            let key = event
                .characters_ignoring_modifiers
                .chars()
                .next()
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| upper.to_ascii_lowercase().to_string());
            (key, format!("Key{upper}"))
        },
        0x10 | 0xA0 | 0xA1 => ("Shift".into(), "ShiftLeft".into()),
        0x11 | 0xA2 | 0xA3 => ("Control".into(), "ControlLeft".into()),
        0x12 | 0xA4 | 0xA5 => ("Alt".into(), "AltLeft".into()),
        0x5B | 0x5C => ("Meta".into(), "MetaLeft".into()),
        0x0D => ("Enter".into(), "Enter".into()),
        0x08 => ("Backspace".into(), "Backspace".into()),
        0x09 => ("Tab".into(), "Tab".into()),
        0x1B => ("Escape".into(), "Escape".into()),
        0x20 => (" ".into(), "Space".into()),
        _ => (
            event
                .characters_ignoring_modifiers
                .chars()
                .next()
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| "Unidentified".into()),
            String::new(),
        ),
    };
    let modifiers = u32::from(event.modifiers.alt)
        | (u32::from(event.modifiers.control) << 1)
        | (u32::from(event.modifiers.meta) << 2)
        | (u32::from(event.modifiers.shift) << 3);
    let mut params = serde_json::json!({
        "type": kind,
        "windowsVirtualKeyCode": event.virtual_key_code,
        "nativeVirtualKeyCode": event.virtual_key_code,
        "code": code, "key": key, "modifiers": modifiers, "autoRepeat": event.is_repeat,
    });
    if let Some(text) = text {
        params["text"] = text.into();
        params["unmodifiedText"] = event.characters_ignoring_modifiers.clone().into();
    }
    params.to_string()
}

macro_rules! forward_native {
    ($name:ident($($arg:ident: $ty:ty),*) -> $result:ty) => {
        fn $name(&mut self, $($arg: $ty),*) -> $result {
            WebSurfaceProducer::$name(&mut *self.0.borrow_mut(), $($arg),*)
        }
    };
}

impl WebSurfaceProducer for SharedNativeProducer {
    fn capabilities(&self) -> native::WebSurfaceCapabilities {
        WebSurfaceProducer::capabilities(&*self.0.borrow())
    }
    fn can_go_back(&self) -> bool {
        WebSurfaceProducer::can_go_back(&*self.0.borrow())
    }
    fn can_go_forward(&self) -> bool {
        WebSurfaceProducer::can_go_forward(&*self.0.borrow())
    }
    forward_native!(acquire_frame() -> Result<native::WebSurfaceFrame, native::WebSurfaceError>);
    fn try_acquire_frame(
        &mut self,
    ) -> Result<Option<native::WebSurfaceFrame>, native::WebSurfaceError> {
        self.drive_keyboard()?;
        WebSurfaceProducer::try_acquire_frame(&mut *self.0.borrow_mut())
    }
    forward_native!(restart_capture_after_stall() -> Result<(), native::WebSurfaceError>);
    forward_native!(resize(size: PhysicalSize<u32>) -> Result<(), native::WebSurfaceError>);
    forward_native!(set_offset(x: f32, y: f32) -> Result<(), native::WebSurfaceError>);
    forward_native!(send_mouse_input(event: native::MouseInput) -> Result<(), native::WebSurfaceError>);
    forward_native!(send_pointer_input(event: native::PointerInput) -> Result<(), native::WebSurfaceError>);
    fn send_keyboard_input(
        &mut self,
        event: native::KeyboardInput,
    ) -> Result<(), native::WebSurfaceError> {
        self.1.borrow_mut().enqueue(keyboard_commands(&event)?)?;
        self.drive_keyboard()
    }
    forward_native!(move_focus(reason: native::FocusReason) -> Result<(), native::WebSurfaceError>);
    forward_native!(poll_cursor_shape() -> Option<native::CursorShape>);
    forward_native!(apply_settings(settings: &native::WebSurfaceSettings) -> Result<(), native::WebSurfaceError>);
    forward_native!(load_url(url: &str) -> Result<(), native::WebSurfaceError>);
    forward_native!(load_html(html: &str) -> Result<(), native::WebSurfaceError>);
    forward_native!(reload() -> Result<(), native::WebSurfaceError>);
    forward_native!(stop() -> Result<(), native::WebSurfaceError>);
    forward_native!(go_back() -> Result<bool, native::WebSurfaceError>);
    forward_native!(go_forward() -> Result<bool, native::WebSurfaceError>);
    forward_native!(set_cookie(cookie: &native::Cookie) -> Result<(), native::WebSurfaceError>);
    forward_native!(request_cookies_for_url(id: native::WebRequestId, url: &str) -> Result<(), native::WebSurfaceError>);
    forward_native!(request_script_result(id: native::WebRequestId, script: &str) -> Result<(), native::WebSurfaceError>);
    fn poll_web_surface_event(&mut self) -> Option<native::WebSurfaceEvent> {
        // Completion errors stay latched for the next fallible acquire/send;
        // this event-only API cannot report SurfaceError.
        let _ = self.drive_keyboard();
        WebSurfaceProducer::poll_web_surface_event(&mut *self.0.borrow_mut())
    }
}

impl WebSurface for TurnstoneScryProducer {
    fn capabilities(&self) -> WebSurfaceCapabilities {
        host_capabilities(self.inner.capabilities())
    }

    fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.inner.navigate_to_url(url)
    }

    fn navigate_to_string(&mut self, html: &str) -> Result<(), SurfaceError> {
        self.inner.navigate_to_string(html)
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.inner.reload()
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.inner.stop()
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_back()
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.inner.go_forward()
    }

    fn can_go_back(&self) -> bool {
        self.inner.can_go_back()
    }

    fn can_go_forward(&self) -> bool {
        self.inner.can_go_forward()
    }

    fn set_cookie(&mut self, cookie: &Cookie) -> Result<(), SurfaceError> {
        self.inner.set_cookie(cookie)
    }

    fn request_cookies_for_url(&mut self, id: WebRequestId, url: &str) -> Result<(), SurfaceError> {
        self.inner.request_cookies_for_url(id, url)
    }

    fn request_script_result(
        &mut self,
        id: WebRequestId,
        script: &str,
    ) -> Result<(), SurfaceError> {
        self.inner.request_script_result(id, script)
    }

    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        self.inner.poll_web_event()
    }
}

fn host_capabilities(mut caps: WebSurfaceCapabilities) -> WebSurfaceCapabilities {
    caps.pointer.mouse = projected_event_capability(
        caps.pointer.mouse,
        "mouse buttons and Shift/Control are forwarded; WebView2 MouseVirtualKeys has no Alt/Meta state",
    );
    caps.document.find_in_page = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not forward document find commands or results",
    );
    caps.document.page_zoom = WebFeatureStatus::unsupported(
        "Scry's Inker adapter forwards legacy zoom settings but not typed page zoom or its outcome",
    );
    caps.document.page_capture = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not implement correlated page capture",
    );
    caps.pdf = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not expose PDF export commands or completions",
    );
    caps.devtools = projected_event_capability(
        caps.devtools,
        "legacy settings toggle DevTools availability; opening DevTools is not forwarded",
    );
    caps.downloads = projected_event_capability(
        caps.downloads,
        "download requests are projected; destination decisions, lifecycle and controls are not",
    );
    caps.popups = projected_event_capability(
        caps.popups,
        "new-window requests are projected; popup-widget surfaces are not",
    );
    caps.permissions = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not retain permission requests or forward their answers",
    );
    caps.auth = WebFeatureStatus::unsupported(
        "Scry's Inker adapter emits authentication diagnostics but does not forward credential answers",
    );
    caps.context_menus = projected_event_capability(
        caps.context_menus,
        "context-menu position and link/image targets are projected; menu items and decisions are not",
    );
    caps.ime_observability = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not project composition or caret geometry",
    );
    caps.accessibility = WebFeatureStatus::unsupported(
        "Scry's Inker adapter does not project an accessibility tree",
    );
    caps.pointer.pen = projected_event_capability(
        caps.pointer.pen,
        "winit 0.30 does not identify pen versus touch in Turnstone's native input path",
    );
    caps.degradation_reasons.push(
        "Turnstone uses captured textures on an owned offscreen HWND; keyboard/text is forwarded through CDP, native OS IME and native rehosting are not integrated".into(),
    );
    caps
}

fn projected_event_capability(status: WebFeatureStatus, projection: &str) -> WebFeatureStatus {
    match status {
        WebFeatureStatus::Supported => WebFeatureStatus::Partial {
            detail: projection.into(),
        },
        WebFeatureStatus::Partial { detail } => WebFeatureStatus::Partial {
            detail: format!("{detail}; {projection}"),
        },
        unsupported => unsupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyboard_event(kind: native::KeyEventKind) -> native::KeyboardInput {
        native::KeyboardInput {
            kind,
            virtual_key_code: 0x41,
            characters: "A".into(),
            characters_ignoring_modifiers: "a".into(),
            modifiers: native::KeyModifierFlags {
                shift: true,
                control: true,
                alt: true,
                meta: true,
                caps_lock: false,
            },
            is_repeat: true,
        }
    }

    fn command_json(command: &str) -> serde_json::Value {
        serde_json::from_str(command).unwrap()
    }

    #[test]
    fn scry_keyboard_preserves_down_char_up_modifiers_and_repeat() {
        let down = keyboard_event(native::KeyEventKind::Down);
        let commands = keyboard_commands(&down).unwrap();
        assert_eq!(commands.len(), 2);
        let key_down = command_json(&commands[0]);
        let character = command_json(&commands[1]);
        assert_eq!(key_down["type"], "rawKeyDown");
        assert!(key_down.get("text").is_none());
        assert_eq!(character["type"], "char");
        assert_eq!(character["text"], "A");
        assert_eq!(character["unmodifiedText"], "a");
        for command in [&key_down, &character] {
            assert_eq!(command["key"], "a");
            assert_eq!(command["code"], "KeyA");
            assert_eq!(command["windowsVirtualKeyCode"], 0x41);
            assert_eq!(command["nativeVirtualKeyCode"], 0x41);
            assert_eq!(command["modifiers"], 15);
            assert_eq!(command["autoRepeat"], true);
        }
        let up = keyboard_commands(&keyboard_event(native::KeyEventKind::Up)).unwrap();
        assert_eq!(up.len(), 1);
        let up = command_json(&up[0]);
        assert_eq!(up["type"], "keyUp");
        assert!(up.get("text").is_none());

        let mut plain = down;
        plain.characters.clear();
        assert_eq!(keyboard_commands(&plain).unwrap().len(), 1);
        plain.characters = "\"\\\nλ🎵".into();
        let commands = keyboard_commands(&plain).unwrap();
        assert_eq!(command_json(&commands[1])["text"], plain.characters);
    }

    #[test]
    fn scry_keyboard_modifier_changes_keep_direction_and_named_keys() {
        for (code, key, modifier, bits) in [
            (
                0x10,
                "Shift",
                native::KeyModifierFlags {
                    shift: true,
                    ..Default::default()
                },
                8,
            ),
            (
                0x11,
                "Control",
                native::KeyModifierFlags {
                    control: true,
                    ..Default::default()
                },
                2,
            ),
            (
                0x12,
                "Alt",
                native::KeyModifierFlags {
                    alt: true,
                    ..Default::default()
                },
                1,
            ),
            (
                0x5B,
                "Meta",
                native::KeyModifierFlags {
                    meta: true,
                    ..Default::default()
                },
                4,
            ),
        ] {
            let mut event = keyboard_event(native::KeyEventKind::ModifiersChanged);
            event.virtual_key_code = code;
            event.modifiers = modifier;
            let down = keyboard_commands(&event).unwrap();
            assert_eq!(down.len(), 1);
            assert_eq!(command_json(&down[0])["type"], "rawKeyDown");
            assert_eq!(command_json(&down[0])["key"], key);
            assert_eq!(command_json(&down[0])["modifiers"], bits);
            event.modifiers = Default::default();
            assert_eq!(
                command_json(&keyboard_commands(&event).unwrap()[0])["type"],
                "keyUp"
            );
        }
        for (code, key, physical_code) in [
            (0x0D, "Enter", "Enter"),
            (0x08, "Backspace", "Backspace"),
            (0x09, "Tab", "Tab"),
            (0x1B, "Escape", "Escape"),
            (0x20, " ", "Space"),
            (0x37, "7", "Digit7"),
        ] {
            let mut event = keyboard_event(native::KeyEventKind::Down);
            event.virtual_key_code = code;
            let command = command_json(&keyboard_commands(&event).unwrap()[0]);
            assert_eq!(command["key"], key);
            assert_eq!(command["code"], physical_code);
        }
    }

    #[test]
    fn scry_keyboard_queue_waits_for_each_completion_before_next_command() {
        let mut queue = KeyboardDispatchQueue::default();
        queue
            .enqueue(keyboard_commands(&keyboard_event(native::KeyEventKind::Down)).unwrap())
            .unwrap();
        queue
            .enqueue(keyboard_commands(&keyboard_event(native::KeyEventKind::Up)).unwrap())
            .unwrap();
        let now = Instant::now();
        let mut submitted = Vec::new();
        submitted.push(command_json(&queue.next(now).unwrap().unwrap())["type"].clone());
        // Repeated host polls cannot send char or keyUp before rawKeyDown's
        // completion. Completion itself does not dispatch anything.
        assert!(queue.next(now).unwrap().is_none());
        assert_eq!(queue.pending.len(), 2);
        queue.complete(Ok(()));
        assert_eq!(queue.pending.len(), 2);
        submitted.push(command_json(&queue.next(now).unwrap().unwrap())["type"].clone());
        assert!(queue.next(now).unwrap().is_none());
        queue.complete(Ok(()));
        submitted.push(command_json(&queue.next(now).unwrap().unwrap())["type"].clone());
        assert!(queue.next(now).unwrap().is_none());
        queue.complete(Ok(()));
        assert!(queue.next(now).unwrap().is_none());
        assert_eq!(submitted, vec!["rawKeyDown", "char", "keyUp"]);
    }

    #[test]
    fn scry_keyboard_queue_failure_and_timeout_retire_following_commands() {
        for failure in [Some("fixture rejected keyboard CDP"), None] {
            let mut queue = KeyboardDispatchQueue::default();
            queue
                .enqueue(keyboard_commands(&keyboard_event(native::KeyEventKind::Down)).unwrap())
                .unwrap();
            let now = Instant::now();
            assert!(queue.next(now).unwrap().is_some());
            if let Some(reason) = failure {
                queue.complete(Err(reason.into()));
            }
            let result = queue.next(now + KEYBOARD_COMPLETION_TIMEOUT);
            assert!(matches!(result, Err(native::WebSurfaceError::Platform(_))));
            assert!(queue.pending.is_empty());
            assert!(queue.in_flight.is_none());
            // Late completions and newly typed keys cannot resume a retired
            // stream; the shell observes this failure through acquire/send.
            queue.complete(Ok(()));
            assert!(queue.enqueue(vec!["late key".into()]).is_err());
            assert!(queue.next(now).is_err());
        }
        assert!(keyboard_completion(Err("native error".into()), "{}").is_err());
        assert!(keyboard_completion(Ok(()), r#"{"error":{"message":"refused"}}"#).is_err());
        assert!(keyboard_completion(Ok(()), "malformed").is_err());
        assert!(keyboard_completion(Ok(()), "{}").is_ok());
    }

    #[test]
    fn scry_keyboard_completion_does_not_retain_retired_producer_state() {
        let queue = Rc::new(RefCell::new(KeyboardDispatchQueue::default()));
        let weak = Rc::downgrade(&queue);
        queue
            .borrow_mut()
            .enqueue(vec!["pending command".into()])
            .unwrap();
        assert!(queue.borrow_mut().next(Instant::now()).unwrap().is_some());
        drop(queue);
        assert!(weak.upgrade().is_none());
    }

    fn mouse_event(
        phase: PointerPhase,
        button: Option<MouseButton>,
        buttons: PointerButtons,
    ) -> PointerEvent {
        PointerEvent {
            pointer_id: 1,
            pointer_type: PointerType::Mouse,
            is_primary: true,
            phase,
            button,
            buttons,
            position: inker::PhysicalPosition { x: 12.0, y: 34.0 },
            width: 1.0,
            height: 1.0,
            pressure: None,
            tangential_pressure: None,
            tilt_x: None,
            tilt_y: None,
            twist: None,
            altitude_angle: None,
            azimuth_angle: None,
            modifiers: inker::KeyboardModifiers {
                shift: true,
                ctrl: true,
                alt: false,
                meta: false,
            },
        }
    }

    #[test]
    fn scry_mouse_buttons_keep_changed_button_and_post_event_state() {
        let right = mouse_pointer_input(&mouse_event(
            PointerPhase::Down,
            Some(MouseButton::Right),
            PointerButtons::PRIMARY | PointerButtons::SECONDARY,
        ))
        .unwrap();
        assert_eq!(right.kind, native::MouseEventKind::RightButtonDown);
        assert_eq!(right.point, (12, 34));
        assert!(right.virtual_keys.left_button && right.virtual_keys.right_button);
        assert!(right.virtual_keys.shift && right.virtual_keys.control);

        let middle_up = mouse_pointer_input(&mouse_event(
            PointerPhase::Up,
            Some(MouseButton::Middle),
            PointerButtons::SECONDARY,
        ))
        .unwrap();
        assert_eq!(middle_up.kind, native::MouseEventKind::MiddleButtonUp);
        assert!(!middle_up.virtual_keys.middle_button);
        assert!(middle_up.virtual_keys.right_button);
        assert!(middle_up.virtual_keys.shift && middle_up.virtual_keys.control);

        let left_up = mouse_pointer_input(&mouse_event(
            PointerPhase::Up,
            Some(MouseButton::Left),
            PointerButtons::NONE,
        ))
        .unwrap();
        assert_eq!(left_up.kind, native::MouseEventKind::LeftButtonUp);
        assert!(!left_up.virtual_keys.left_button);
        assert!(
            mouse_pointer_input(&mouse_event(PointerPhase::Down, None, PointerButtons::NONE))
                .is_err()
        );
    }

    #[test]
    fn scry_mouse_move_leave_and_auxiliary_buttons_use_native_mouse_route() {
        let moved = mouse_pointer_input(&mouse_event(
            PointerPhase::Move,
            None,
            PointerButtons::PRIMARY,
        ))
        .unwrap();
        assert_eq!(moved.kind, native::MouseEventKind::Move);
        assert!(moved.virtual_keys.left_button);
        let leave = mouse_pointer_input(&mouse_event(
            PointerPhase::Cancel,
            None,
            PointerButtons::NONE,
        ))
        .unwrap();
        assert_eq!(leave.kind, native::MouseEventKind::Leave);
        assert_eq!(leave.point, (12, 34));
        for (button, index, mask) in [
            (MouseButton::Back, 1, PointerButtons::BACK),
            (MouseButton::Forward, 2, PointerButtons::FORWARD),
        ] {
            let down =
                mouse_pointer_input(&mouse_event(PointerPhase::Down, Some(button), mask)).unwrap();
            assert_eq!(down.kind, native::MouseEventKind::XButtonDown);
            assert_eq!(down.mouse_data, index);
            assert_eq!(down.virtual_keys.x_button1, index == 1);
            assert_eq!(down.virtual_keys.x_button2, index == 2);
            let up = mouse_pointer_input(&mouse_event(
                PointerPhase::Up,
                Some(button),
                PointerButtons::NONE,
            ))
            .unwrap();
            assert_eq!(up.kind, native::MouseEventKind::XButtonUp);
            assert_eq!(up.mouse_data, index);
            assert!(!up.virtual_keys.x_button1 && !up.virtual_keys.x_button2);
        }
    }

    #[test]
    fn scry_profile_requires_explicit_absolute_path() {
        assert!(profile_path("").is_err());
        assert!(profile_path("relative-profile").is_err());
        let path = PathBuf::from(r"C:\turnstone-data\scry\profiles\node-7");
        assert_eq!(profile_path(path.to_str().unwrap()).unwrap(), path);
    }

    #[test]
    fn scry_factory_refuses_other_threads_before_native_construction() {
        let owner = std::thread::current().id();
        assert!(require_owner_thread(owner).is_ok());
        let result = std::thread::spawn(move || require_owner_thread(owner))
            .join()
            .unwrap();
        assert!(matches!(result, Err(SurfaceError::SpawnFailed(reason))
            if reason.contains("UI thread")));
    }

    #[test]
    fn scry_host_capabilities_refuse_unforwarded_adapter_commands() {
        let mut caps = WebSurfaceCapabilities::default();
        caps.document.find_in_page = WebFeatureStatus::Supported;
        caps.document.page_zoom = WebFeatureStatus::Supported;
        caps.auth = WebFeatureStatus::Supported;
        caps.permissions = WebFeatureStatus::Supported;
        caps.pdf = WebFeatureStatus::Supported;
        caps.ime_observability = WebFeatureStatus::Supported;
        caps.accessibility = WebFeatureStatus::Supported;
        caps.script.result = WebFeatureStatus::Supported;
        caps.cookie.read = WebFeatureStatus::Supported;
        let caps = host_capabilities(caps);
        for status in [
            caps.document.find_in_page,
            caps.document.page_zoom,
            caps.document.page_capture,
            caps.auth,
            caps.permissions,
            caps.pdf,
            caps.ime_observability,
            caps.accessibility,
        ] {
            assert!(matches!(status, WebFeatureStatus::Unsupported { reason }
                if !reason.is_empty()));
        }
        assert_eq!(caps.script.result, WebFeatureStatus::Supported);
        assert_eq!(caps.cookie.read, WebFeatureStatus::Supported);
    }

    #[test]
    fn scry_host_capabilities_preserve_refusal_and_partial_event_limits() {
        let mut caps = WebSurfaceCapabilities::default();
        caps.downloads = WebFeatureStatus::unsupported("backend denied downloads");
        caps.popups = WebFeatureStatus::Partial {
            detail: "backend popup limit".into(),
        };
        caps.context_menus = WebFeatureStatus::Supported;
        let caps = host_capabilities(caps);
        assert!(
            matches!(caps.downloads, WebFeatureStatus::Unsupported { reason }
            if reason == "backend denied downloads")
        );
        assert!(matches!(caps.popups, WebFeatureStatus::Partial { detail }
            if detail.contains("backend popup limit") && detail.contains("popup-widget")));
        assert!(
            matches!(caps.context_menus, WebFeatureStatus::Partial { detail }
            if detail.contains("menu items and decisions"))
        );
    }
}
