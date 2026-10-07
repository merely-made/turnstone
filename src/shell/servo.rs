// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Upstream Servo views on Turnstone's existing wgpu device.
//!
//! The process root lives on the UI thread, including between the last closed
//! view and a later reopen. Upstream options and JS initialization are process
//! scoped. Each view has its own rendering context and one ordered delegate
//! queue. The shell must release producers and cached frames before shutdown.

use graft_engine::{GraftFrame, GraftProducerFactory, GraftSurface};
use grafting::{ImportedTexture, TextureOrigin};
use inker::{
    CursorShape, DragEvent, DragOperationSet, FocusReason, KeyboardEvent, MouseButton, MouseEvent,
    MouseEventKind, NativeTextureHandle, NavigationEvent, OwnedSurfaceFrame, PhysicalPosition,
    PointerEvent, PointerPhase, PointerType, SurfaceError, SurfaceSettings, SurfaceSpawnRequest,
    SurfaceSyncHandle, SurfaceTextureFormat, WebFeatureStatus, WebFrameTransportMode, WebMessage,
    WebSurfaceCapabilities, WebSurfaceEvent,
};
use servo::{
    DevicePoint, EventLoopWaker, InputEvent, Key, KeyState, LoadStatus, Location, Modifiers,
    MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent, MouseMoveEvent, NamedKey, Opts,
    PermissionRequest, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate, WebViewPoint,
    WheelDelta, WheelEvent, WheelMode,
};
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

pub(super) const FRAME_KIND: &str = "turnstone.servo.imported-frame";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ServoHostOptions {
    pub profile_name: String,
    pub profile_dir: PathBuf,
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
}

#[derive(Clone)]
struct Waker(Arc<dyn Fn() + Send + Sync>);
impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }
    fn wake(&self) {
        (self.0)();
    }
}

pub(super) struct TurnstoneServoFactory {
    owner: ThreadId,
    options: ServoHostOptions,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl TurnstoneServoFactory {
    pub(super) fn profile_name(&self) -> &str {
        &self.options.profile_name
    }
    pub(super) fn profile_dir(&self) -> &std::path::Path {
        &self.options.profile_dir
    }
    /// Construct exactly once on the UI thread. The configurable named profile
    /// is shared by this process's Servo views, independently of node profiles.
    pub(super) fn new(
        options: ServoHostOptions,
        device: wgpu::Device,
        queue: wgpu::Queue,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, SurfaceError> {
        if options.profile_name.trim().is_empty() || options.profile_dir.as_os_str().is_empty() {
            return Err(spawn_error(
                "Servo needs an explicitly named profile directory",
            ));
        }
        let owner = std::thread::current().id();
        if PROCESS_OWNER.get().is_some_and(|actual| *actual != owner) {
            return Err(spawn_error(
                "Servo must be constructed on its original UI thread",
            ));
        }
        ROOT.with(|slot| {
            if slot.borrow().is_some() || INITIALIZED.with(Cell::get) {
                return Err(spawn_error(
                    "Servo process root is already initialized; reuse its factory",
                ));
            }
            std::fs::create_dir_all(&options.profile_dir)
                .map_err(|error| spawn_error(error.to_string()))?;
            let opts = Opts {
                config_dir: Some(options.profile_dir.clone()),
                ..Opts::default()
            };
            // Record before upstream initialization: its global options cannot
            // safely be retried, even if construction unwinds.
            if PROCESS_OWNER.set(owner).is_err() {
                require_owner(*PROCESS_OWNER.get().expect("Servo owner was assigned"))?;
            }
            INITIALIZED.with(|initialized| initialized.set(true));
            if rustls::crypto::CryptoProvider::get_default().is_none() {
                // An existing process provider wins, including an install
                // racing this one. Servo's net initialization needs a default.
                let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
            }
            let servo = ServoBuilder::default()
                .opts(opts)
                .event_loop_waker(Box::new(Waker(wake.clone())))
                .build();
            *slot.borrow_mut() = Some(Rc::new(ProcessRoot {
                servo,
                options: options.clone(),
                views: Cell::new(0),
                wake,
            }));
            Ok(Self {
                owner,
                options,
                device,
                queue,
            })
        })
    }

    fn root(&self) -> Result<Rc<ProcessRoot>, SurfaceError> {
        require_owner(self.owner)?;
        ROOT.with(|slot| slot.borrow().clone())
            .ok_or_else(|| spawn_error("Servo process root is shut down"))
    }
}

pub(super) fn pump() -> Result<(), SurfaceError> {
    if let Some(owner) = PROCESS_OWNER.get() {
        require_owner(*owner)?;
    }
    let root = ROOT.with(|slot| slot.borrow().clone());
    if let Some(root) = root {
        root.servo.spin_event_loop();
    }
    Ok(())
}

pub(super) fn active_views() -> usize {
    ROOT.with(|slot| slot.borrow().as_ref().map_or(0, |root| root.views.get()))
}

/// Release the process root after all producers and imported caches are gone.
/// Reinitialization is refused for the remainder of this process.
pub(super) fn shutdown() -> Result<(), SurfaceError> {
    if let Some(owner) = PROCESS_OWNER.get() {
        require_owner(*owner)?;
    }
    ROOT.with(|slot| {
        if slot
            .borrow()
            .as_ref()
            .is_some_and(|root| root.views.get() != 0)
        {
            return Err(SurfaceError::Busy {
                operation: "Servo shutdown with live views".into(),
            });
        }
        let root = slot.borrow_mut().take();
        drop(root);
        Ok(())
    })
}

impl GraftProducerFactory for TurnstoneServoFactory {
    fn build(&self, request: &SurfaceSpawnRequest) -> Result<Box<dyn GraftSurface>, SurfaceError> {
        let root = self.root()?;
        if root.options != self.options {
            return Err(spawn_error(
                "Servo named process profile differs from the initialized root",
            ));
        }
        // The shell resolves Servo requests to the explicitly selected process
        // profile. Refuse accidentally forwarding a per-node binding.
        if PathBuf::from(&request.profile.user_data_dir) != self.options.profile_dir {
            return Err(spawn_error(
                "Servo view requested a different process profile",
            ));
        }
        if request.fence_handle.is_some() {
            return Err(spawn_error(
                "Servo import owns synchronization; raw spawn fences are unsupported",
            ));
        }
        let url = Url::parse(&request.url).map_err(|error| spawn_error(error.to_string()))?;
        let interop = ServoWgpuInteropAdapter::new(
            self.device.clone(),
            self.queue.clone(),
            PhysicalSize::new(request.width.max(1), request.height.max(1)),
        )
        .map_err(|error| spawn_error(error.to_string()))?;
        let delegate = Rc::new(Delegate {
            events: RefCell::new(VecDeque::new()),
            ready: Cell::new(false),
            animating: Cell::new(false),
            requested_url: RefCell::new(request.url.clone()),
            wake: root.wake.clone(),
        });
        let view = WebViewBuilder::new(&root.servo, interop.rendering_context())
            .url(url)
            .delegate(delegate.clone())
            .build();
        root.views.set(root.views.get() + 1);
        Ok(Box::new(TurnstoneServoSurface {
            view: Some(view),
            interop: Some(interop),
            delegate,
            root,
            frame_epoch: 0,
            device: self.device.clone(),
            queue: self.queue.clone(),
            pressed_keys: HashMap::new(),
        }))
    }
}

#[derive(Debug)]
pub(super) struct ServoImportedFrame {
    pub imported: ImportedTexture,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub resource_epoch: u64,
}
impl OwnedSurfaceFrame for ServoImportedFrame {
    fn payload_kind(&self) -> &'static str {
        FRAME_KIND
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

struct Delegate {
    events: RefCell<VecDeque<WebSurfaceEvent>>,
    ready: Cell<bool>,
    animating: Cell<bool>,
    requested_url: RefCell<String>,
    wake: Arc<dyn Fn() + Send + Sync>,
}
impl Delegate {
    fn push(&self, event: WebSurfaceEvent) {
        self.events.borrow_mut().push_back(event);
        (self.wake)();
    }
}
impl WebViewDelegate for Delegate {
    fn request_permission(&self, _: WebView, request: PermissionRequest) {
        // This host slice exposes no permission UI or decision persistence.
        // Resolve explicitly rather than inherit a caller-selected fallback.
        request.deny();
    }
    fn notify_new_frame_ready(&self, _: WebView) {
        self.ready.set(true);
        (self.wake)();
    }
    fn notify_animating_changed(&self, _: WebView, animating: bool) {
        self.animating.set(animating);
        (self.wake)();
    }
    fn notify_url_changed(&self, _: WebView, url: Url) {
        *self.requested_url.borrow_mut() = url.to_string();
        self.push(WebSurfaceEvent::AddressChanged {
            url: url.to_string(),
        });
    }
    fn notify_page_title_changed(&self, _: WebView, title: Option<String>) {
        self.push(WebSurfaceEvent::TitleChanged {
            title: title.unwrap_or_default(),
        });
    }
    fn notify_load_status_changed(&self, view: WebView, status: LoadStatus) {
        let url = self.requested_url.borrow().clone();
        let event = match status {
            LoadStatus::Started => NavigationEvent::Started { url },
            LoadStatus::HeadParsed => NavigationEvent::Committed { url },
            LoadStatus::Complete => NavigationEvent::Finished {
                url,
                title: view.page_title(),
            },
        };
        self.push(WebSurfaceEvent::Navigation(event));
    }
    fn notify_crashed(&self, _: WebView, reason: String, _: Option<String>) {
        self.push(WebSurfaceEvent::Navigation(NavigationEvent::Failed {
            url: self.requested_url.borrow().clone(),
            reason,
        }));
    }
}

struct TurnstoneServoSurface {
    view: Option<WebView>,
    interop: Option<ServoWgpuInteropAdapter>,
    delegate: Rc<Delegate>,
    root: Rc<ProcessRoot>,
    frame_epoch: u64,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pressed_keys: HashMap<u32, Key>,
}
impl TurnstoneServoSurface {
    fn view(&self) -> &WebView {
        self.view.as_ref().expect("live Servo surface has a view")
    }
}
impl Drop for TurnstoneServoSurface {
    fn drop(&mut self) {
        drop(self.view.take());
        self.root.servo.spin_event_loop();
        drop(self.interop.take());
        self.root.views.set(self.root.views.get() - 1);
    }
}

impl GraftSurface for TurnstoneServoSurface {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.view()
            .resize(PhysicalSize::new(width.max(1), height.max(1)));
        Ok(())
    }
    fn acquire_frame(&mut self) -> Result<Option<GraftFrame>, SurfaceError> {
        self.root.servo.spin_event_loop();
        if !self.delegate.ready.replace(false) && !self.delegate.animating.get() {
            return Ok(None);
        }
        self.view().paint();
        // Upstream WebView::paint draws without presenting. Present through
        // the importing context so its hook runs before the GL buffer swap.
        self.interop.as_ref().unwrap().rendering_context().present();
        let imported = self
            .interop
            .as_ref()
            .unwrap()
            .take_imported_texture()
            .ok_or_else(|| {
                SurfaceError::FrameAcquisitionFailed(
                    "Servo pre-present import produced no frame".into(),
                )
            })?;
        let actual_size = imported.texture.size();
        if imported.origin != TextureOrigin::TopLeft
            || imported.format != wgpu::TextureFormat::Rgba8Unorm
            || imported.texture.format() != imported.format
            || imported.texture.dimension() != wgpu::TextureDimension::D2
            || imported.size.width == 0
            || imported.size.height == 0
            || (
                actual_size.width,
                actual_size.height,
                actual_size.depth_or_array_layers,
            ) != (imported.size.width, imported.size.height, 1)
            || !imported
                .texture
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(SurfaceError::FrameAcquisitionFailed(
                "Servo import disagrees with its normalized texture metadata".into(),
            ));
        }
        self.frame_epoch = self.frame_epoch.checked_add(1).ok_or_else(|| {
            SurfaceError::FrameAcquisitionFailed("Servo frame epoch exhausted".into())
        })?;
        let (width, height) = (imported.size.width, imported.size.height);
        Ok(Some(GraftFrame {
            texture: NativeTextureHandle::OwnedPayload(Box::new(ServoImportedFrame {
                imported,
                device: self.device.clone(),
                queue: self.queue.clone(),
                resource_epoch: self.frame_epoch,
            })),
            sync: SurfaceSyncHandle::None,
            width,
            height,
            format: SurfaceTextureFormat::Rgba8Unorm,
            resource_epoch: self.frame_epoch,
        }))
    }
    fn load_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        let url =
            Url::parse(url).map_err(|error| SurfaceError::NavigationFailed(error.to_string()))?;
        *self.delegate.requested_url.borrow_mut() = url.to_string();
        self.view().load(url);
        Ok(())
    }
    fn load_html(&mut self, _: &str) -> Result<(), SurfaceError> {
        Err(unsupported("inline HTML loading"))
    }
    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.view().reload();
        Ok(())
    }
    fn stop(&mut self) -> Result<(), SurfaceError> {
        Err(unsupported("stop loading"))
    }
    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.view().go_back(1);
        Ok(())
    }
    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.view().go_forward(1);
        Ok(())
    }
    fn can_go_back(&self) -> bool {
        self.view().can_go_back()
    }
    fn can_go_forward(&self) -> bool {
        self.view().can_go_forward()
    }
    fn notify_mouse(&mut self, event: MouseEvent) -> Result<(), SurfaceError> {
        let point = WebViewPoint::Device(DevicePoint::new(
            event.position.x as f32,
            event.position.y as f32,
        ));
        let event = match event.kind {
            MouseEventKind::Moved => InputEvent::MouseMove(MouseMoveEvent::new(point)),
            MouseEventKind::Pressed | MouseEventKind::Released => {
                let button = match event.button.ok_or_else(|| {
                    SurfaceError::InputFailed("Servo mouse button is absent".into())
                })? {
                    MouseButton::Left => servo::MouseButton::Left,
                    MouseButton::Middle => servo::MouseButton::Middle,
                    MouseButton::Right => servo::MouseButton::Right,
                    MouseButton::Back => servo::MouseButton::Back,
                    MouseButton::Forward => servo::MouseButton::Forward,
                };
                let action = if matches!(event.kind, MouseEventKind::Pressed) {
                    MouseButtonAction::Down
                } else {
                    MouseButtonAction::Up
                };
                InputEvent::MouseButton(MouseButtonEvent::new(action, button, point))
            },
            MouseEventKind::ScrollPixels { delta_x, delta_y }
            | MouseEventKind::ScrollLines { delta_x, delta_y } => {
                let mode = if matches!(event.kind, MouseEventKind::ScrollLines { .. }) {
                    WheelMode::DeltaLine
                } else {
                    WheelMode::DeltaPixel
                };
                InputEvent::Wheel(WheelEvent::new(
                    WheelDelta {
                        x: delta_x as f64,
                        y: delta_y as f64,
                        z: 0.0,
                        mode,
                    },
                    point,
                ))
            },
        };
        self.view().notify_input_event(event);
        Ok(())
    }
    fn notify_pointer(&mut self, event: PointerEvent) -> Result<(), SurfaceError> {
        if event.pointer_type != PointerType::Mouse {
            return Err(unsupported("pen/touch pointer projection"));
        }
        let kind = match event.phase {
            PointerPhase::Down => MouseEventKind::Pressed,
            PointerPhase::Move => MouseEventKind::Moved,
            PointerPhase::Up => MouseEventKind::Released,
            PointerPhase::Cancel => {
                self.view()
                    .notify_input_event(InputEvent::MouseLeftViewport(
                        MouseLeftViewportEvent::default(),
                    ));
                return Ok(());
            },
        };
        self.notify_mouse(MouseEvent {
            position: event.position,
            button: event.button,
            kind,
        })
    }
    fn notify_drag(&mut self, _: DragEvent) -> Result<(), SurfaceError> {
        Err(unsupported("HTML drag"))
    }
    fn finish_drag_source(
        &mut self,
        _: PhysicalPosition,
        _: DragOperationSet,
    ) -> Result<(), SurfaceError> {
        Err(unsupported("HTML drag source completion"))
    }
    fn notify_keyboard(&mut self, event: KeyboardEvent) -> Result<(), SurfaceError> {
        let key = logical_key(&event, &mut self.pressed_keys);
        let mut modifiers = Modifiers::empty();
        modifiers.set(Modifiers::SHIFT, event.modifiers.shift);
        modifiers.set(Modifiers::CONTROL, event.modifiers.ctrl);
        modifiers.set(Modifiers::ALT, event.modifiers.alt);
        modifiers.set(Modifiers::META, event.modifiers.meta);
        self.view().notify_input_event(InputEvent::Keyboard(
            servo::KeyboardEvent::new_without_event(
                if event.pressed {
                    KeyState::Down
                } else {
                    KeyState::Up
                },
                key,
                servo::Code::Unidentified,
                Location::Standard,
                modifiers,
                false,
                false,
            ),
        ));
        Ok(())
    }
    fn focus(&mut self, _: FocusReason) -> Result<(), SurfaceError> {
        self.view().focus();
        Ok(())
    }
    fn poll_navigation_event(&mut self) -> Option<NavigationEvent> {
        None
    }
    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        None
    }
    fn poll_web_message(&mut self) -> Option<WebMessage> {
        None
    }
    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        self.delegate.events.borrow_mut().pop_front()
    }
    fn web_capabilities(&self) -> WebSurfaceCapabilities {
        let mut caps = WebSurfaceCapabilities {
            backend_name: "graft.servo".into(),
            frame_transport: WebFrameTransportMode::ImportedTexture,
            ..WebSurfaceCapabilities::default()
        };
        caps.document.navigation = WebFeatureStatus::Partial {
            detail: "native load/reload/history; upstream has no stop API".into(),
        };
        caps.document.page_zoom = WebFeatureStatus::Partial {
            detail: "requested native zoom; effective readback projection is not wired".into(),
        };
        caps.pointer.mouse = WebFeatureStatus::Partial {
            detail:
                "basic down/move/up/cancel; pointer modifiers and rich fields are not projected"
                    .into(),
        };
        caps.degradation_reasons.push("first Servo slice: physical key codes, rich pointer fields, scripts, cookies, permissions and browser tools remain unavailable".into());
        caps
    }
    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        if settings.dev_tools {
            return Err(unsupported("DevTools"));
        }
        if settings.background_color != SurfaceSettings::default().background_color {
            return Err(unsupported("host background override"));
        }
        self.view().set_page_zoom(settings.zoom_factor as f32);
        Ok(())
    }
}

fn logical_key(event: &KeyboardEvent, pressed_keys: &mut HashMap<u32, Key>) -> Key {
    if !event.pressed {
        if let Some(key) = pressed_keys.remove(&event.key_code) {
            return key;
        }
    }
    // Winit can supply an empty string for a non-printable key, or control
    // text such as carriage return for Enter. Neither is a character key.
    let key = if let Some(text) = event
        .text
        .as_ref()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
    {
        Key::Character(text.clone())
    } else if event.key_code == 32
        || (48..=57).contains(&event.key_code)
        || (65..=90).contains(&event.key_code)
    {
        Key::Character(
            char::from_u32(event.key_code)
                .unwrap()
                .to_ascii_lowercase()
                .to_string(),
        )
    } else {
        Key::Named(match event.key_code {
            8 => NamedKey::Backspace,
            9 => NamedKey::Tab,
            13 => NamedKey::Enter,
            27 => NamedKey::Escape,
            33 => NamedKey::PageUp,
            34 => NamedKey::PageDown,
            35 => NamedKey::End,
            36 => NamedKey::Home,
            37 => NamedKey::ArrowLeft,
            38 => NamedKey::ArrowUp,
            39 => NamedKey::ArrowRight,
            40 => NamedKey::ArrowDown,
            45 => NamedKey::Insert,
            46 => NamedKey::Delete,
            _ => NamedKey::Unidentified,
        })
    };
    if event.pressed {
        pressed_keys.insert(event.key_code, key.clone());
    }
    key
}

fn require_owner(owner: ThreadId) -> Result<(), SurfaceError> {
    if std::thread::current().id() == owner {
        Ok(())
    } else {
        Err(spawn_error("Servo operation requires its UI thread"))
    }
}
fn spawn_error(detail: impl Into<String>) -> SurfaceError {
    SurfaceError::SpawnFailed(detail.into())
}
fn unsupported(operation: &str) -> SurfaceError {
    SurfaceError::Unsupported(format!("Servo host does not support {operation}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(key_code: u32, pressed: bool, text: Option<&str>) -> KeyboardEvent {
        KeyboardEvent {
            key_code,
            scan_code: 0,
            modifiers: inker::KeyboardModifiers::default(),
            pressed,
            text: text.map(str::to_owned),
        }
    }

    #[test]
    fn empty_and_control_text_preserve_named_keys_and_matching_release() {
        for (code, named) in [
            (8, NamedKey::Backspace),
            (9, NamedKey::Tab),
            (13, NamedKey::Enter),
            (37, NamedKey::ArrowLeft),
            (39, NamedKey::ArrowRight),
        ] {
            let mut pressed = HashMap::new();
            assert_eq!(
                logical_key(&event(code, true, Some("")), &mut pressed),
                Key::Named(named)
            );
            assert_eq!(
                logical_key(&event(code, false, None), &mut pressed),
                Key::Named(named)
            );
            assert!(pressed.is_empty());
        }
        let mut pressed = HashMap::new();
        assert_eq!(
            logical_key(&event(13, true, Some("\r")), &mut pressed),
            Key::Named(NamedKey::Enter)
        );
    }

    #[test]
    fn unicode_text_is_preserved_on_release_without_text() {
        let mut pressed = HashMap::new();
        let expected = Key::Character("é中".into());
        assert_eq!(
            logical_key(&event(0, true, Some("é中")), &mut pressed),
            expected
        );
        assert_eq!(logical_key(&event(0, false, None), &mut pressed), expected);
        assert!(pressed.is_empty());
    }
}
