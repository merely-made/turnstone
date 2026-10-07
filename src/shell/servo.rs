// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Upstream Servo views on Turnstone's existing wgpu device.
//!
//! The process root lives on the UI thread, including between the last closed
//! view and a later reopen. Upstream options and JS initialization are process
//! scoped. Each view has its own rendering context and one ordered delegate
//! queue. The shell must release producers and cached frames before shutdown.

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
    MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent, MouseMoveEvent, NamedKey, Opts,
    PermissionRequest, Preferences, Servo, ServoBuilder, WebView, WebViewBuilder, WebViewDelegate,
    WebViewPoint, WheelDelta, WheelEvent, WheelMode,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum A11yFocusPolicy {
    Upstream,
    DocumentRoot,
}

impl A11yFocusPolicy {
    fn name(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::DocumentRoot => "document-root",
        }
    }

    fn project(self, update: &mut SurfaceAccessibilityUpdate) {
        // Pinned AAC exports a fixed placeholder, not DOM keyboard focus.
        // This explicit partial projection uses AccessKit's document-root
        // representation; it never supplies missing tree metadata or nodes.
        if self == Self::DocumentRoot {
            if let Some(tree) = &update.tree {
                update.focus = tree.root;
            }
        }
    }
}

fn parse_a11y_focus_policy(value: Option<&str>) -> Result<A11yFocusPolicy, SurfaceError> {
    match value {
        None | Some("document-root") => Ok(A11yFocusPolicy::DocumentRoot),
        Some("upstream") => Ok(A11yFocusPolicy::Upstream),
        Some(_) => Err(spawn_error(
            "TURNSTONE_SERVO_A11Y_FOCUS must be upstream or document-root",
        )),
    }
}

fn requested_a11y_focus_policy() -> Result<A11yFocusPolicy, SurfaceError> {
    match std::env::var("TURNSTONE_SERVO_A11Y_FOCUS") {
        Ok(value) => parse_a11y_focus_policy(Some(&value)),
        Err(std::env::VarError::NotPresent) => parse_a11y_focus_policy(None),
        Err(_) => Err(spawn_error(
            "TURNSTONE_SERVO_A11Y_FOCUS is not valid Unicode",
        )),
    }
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
    a11y_focus_policy: A11yFocusPolicy,
}

impl TurnstoneServoFactory {
    pub(super) fn profile_name(&self) -> &str {
        &self.options.profile_name
    }
    pub(super) fn profile_dir(&self) -> &std::path::Path {
        &self.options.profile_dir
    }
    pub(super) fn a11y_focus_policy(&self) -> &'static str {
        self.a11y_focus_policy.name()
    }
    /// Construct exactly once on the UI thread. The configurable named profile
    /// is shared by this process's Servo views, independently of node profiles.
    pub(super) fn new(
        options: ServoHostOptions,
        device: wgpu::Device,
        queue: wgpu::Queue,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, SurfaceError> {
        // Refuse unknown settings before touching upstream process globals.
        // Factory reuse and every later view retain this one selection.
        let a11y_focus_policy = requested_a11y_focus_policy()?;
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
                .preferences(Preferences {
                    accessibility_enabled: true,
                    ..Preferences::default()
                })
                .event_loop_waker(Box::new(Waker(wake.clone())))
                .build();
            *slot.borrow_mut() = Some(Rc::new(ProcessRoot {
                servo,
                options: options.clone(),
                views: Cell::new(0),
                wake,
                a11y_focus_policy,
            }));
            Ok(Self {
                owner,
                options,
                device,
                queue,
                a11y_focus_policy,
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
        if root.a11y_focus_policy != self.a11y_focus_policy {
            return Err(spawn_error(
                "Servo accessibility process policy differs from its factory",
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
        let interop = ServoWgpuInteropAdapter::new_with_diagnostic_sync(
            self.device.clone(),
            self.queue.clone(),
            PhysicalSize::new(request.width.max(1), request.height.max(1)),
            requested_gpu_sync()?,
        )
        .map_err(|error| spawn_error(error.to_string()))?;
        let delegate = Rc::new(Delegate {
            events: RefCell::new(VecDeque::new()),
            accessibility_updates: RefCell::new(VecDeque::new()),
            accessibility_active: Cell::new(false),
            accessibility_focus_provenance: Cell::new(0),
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
    accessibility_updates: RefCell<VecDeque<SurfaceAccessibilityUpdate>>,
    accessibility_active: Cell<bool>,
    accessibility_focus_provenance: Cell<u32>,
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

    fn push_accessibility_update(&self, update: SurfaceAccessibilityUpdate) {
        if !self.accessibility_active.get() {
            return;
        }
        // Servo sends its wrapper graft before the document's update. Keep
        // every nested TreeId and the callback order intact for host admission.
        self.accessibility_updates.borrow_mut().push_back(update);
        (self.wake)();
    }

    fn deactivate_accessibility(&self) {
        self.accessibility_active.set(false);
        self.accessibility_updates.borrow_mut().clear();
        self.accessibility_focus_provenance.set(0);
    }

    fn poll_projected_accessibility_update(
        &self,
        policy: A11yFocusPolicy,
    ) -> Option<SurfaceAccessibilityUpdate> {
        // The FIFO always holds the exact raw supplier callback first. Change
        // only focus at this producer boundary, preserving supplier identities,
        // graph, bounds and advertised actions. DOM focused-node support and
        // assistive actions remain unsupported.
        let mut update = self.accessibility_updates.borrow_mut().pop_front()?;
        let supplier_focus = update.focus;
        policy.project(&mut update);
        // Bound provenance to the first 32 callbacks per activation. Metadata
        // alone is logged; labels, values and page text are never included.
        let count = self.accessibility_focus_provenance.get();
        if count < 32 {
            tracing::debug!(target: "turnstone::servo::a11y",
                tree_id = %update.tree_id.0,
                supplier_focus = supplier_focus.0,
                projected_focus = update.focus.0,
                policy = policy.name(),
                "Servo accessibility focus projection");
            self.accessibility_focus_provenance.set(count + 1);
        }
        Some(update)
    }
}
impl WebViewDelegate for Delegate {
    fn notify_accessibility_tree_update(&self, _: WebView, update: servo::accesskit::TreeUpdate) {
        self.push_accessibility_update(update);
    }
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
        self.delegate.deactivate_accessibility();
        if let Some(view) = &self.view {
            view.set_accessibility_active(false);
        }
        drop(self.view.take());
        self.root.servo.spin_event_loop();
        drop(self.interop.take());
        self.root.views.set(self.root.views.get() - 1);
    }
}

impl GraftSurface for TurnstoneServoSurface {
    fn set_accessibility_active(
        &mut self,
        active: bool,
    ) -> Result<Option<SurfaceAccessibilityTreeId>, SurfaceError> {
        if !active {
            self.delegate.deactivate_accessibility();
            self.view().set_accessibility_active(false);
            return Ok(None);
        }
        // Callbacks are queued even if upstream invokes the delegate before
        // this returns. The host admits the returned root before draining them.
        self.delegate.accessibility_active.set(true);
        match self.view().set_accessibility_active(true) {
            Some(tree) => Ok(Some(tree)),
            None => {
                self.delegate.deactivate_accessibility();
                Err(unsupported("accessibility activation"))
            },
        }
    }

    fn poll_accessibility_update(&mut self) -> Option<SurfaceAccessibilityUpdate> {
        self.delegate
            .poll_projected_accessibility_update(self.root.a11y_focus_policy)
    }

    fn request_accessibility_resync(&mut self) -> Result<SurfaceAccessibilityTreeId, SurfaceError> {
        // AAC has no public resend API. Reactivation requests initial trees
        // and creates a new wrapper TreeId. The host retires the old forest
        // and admits this root before polling the replacement's callbacks.
        // Upstream deactivation is asynchronous; the host still validates the
        // replacement's complete initial tree before accepting its deltas.
        self.set_accessibility_active(false)?;
        self.set_accessibility_active(true)?
            .ok_or_else(|| unsupported("accessibility resynchronization"))
    }

    fn send_accessibility_action(
        &mut self,
        _request: SurfaceAccessibilityActionRequest,
    ) -> Result<(), SurfaceError> {
        // AAC's DOM bridge implements Click, but its node producer advertises
        // no actions. Forwarding Click without that mask would invent support;
        // Focus, value/edit, selection and scrolling lack DOM handlers too.
        Err(unsupported("unadvertised accessibility actions"))
    }

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
            .take_imported_texture_result()
            .map_err(|error| SurfaceError::FrameAcquisitionFailed(error.to_string()))?
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
                    MouseButton::Left => servo::MouseButton::Primary,
                    MouseButton::Middle => servo::MouseButton::Auxiliary,
                    MouseButton::Right => servo::MouseButton::Secondary,
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
        self.view().set_focused(true);
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

fn requested_gpu_sync() -> Result<DiagnosticGpuSync, SurfaceError> {
    fn option(name: &str) -> Result<Option<String>, SurfaceError> {
        match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(_) => Err(spawn_error(format!("{name} must be Unicode"))),
        }
    }
    let mode = option("TURNSTONE_SERVO_GPU_SYNC")?;
    let timeout = option("TURNSTONE_SERVO_GPU_SYNC_TIMEOUT_MS")?
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|_| spawn_error("Servo GPU diagnostic timeout must be positive milliseconds"))?
        .unwrap_or(5000);
    if timeout == 0 {
        return Err(spawn_error(
            "Servo GPU diagnostic timeout must be positive milliseconds",
        ));
    }
    parse_gpu_sync(
        mode.as_deref().unwrap_or("existing"),
        std::time::Duration::from_millis(timeout),
    )
}

fn parse_gpu_sync(
    mode: &str,
    timeout: std::time::Duration,
) -> Result<DiagnosticGpuSync, SurfaceError> {
    // These explicit diagnostics preserve the existing production policy.
    // Native runners guard the process because GL producer completion has no timeout.
    match mode {
        "existing" => Ok(DiagnosticGpuSync::Existing),
        "producer" => Ok(DiagnosticGpuSync::ProducerCompletion),
        "normalization" => Ok(DiagnosticGpuSync::NormalizationCompletion { timeout }),
        "both" => Ok(DiagnosticGpuSync::Both { timeout }),
        _ => Err(spawn_error(
            "TURNSTONE_SERVO_GPU_SYNC must be existing, producer, normalization or both",
        )),
    }
}
fn unsupported(operation: &str) -> SurfaceError {
    SurfaceError::Unsupported(format!("Servo host does not support {operation}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accessibility_delegate() -> Delegate {
        Delegate {
            events: RefCell::new(VecDeque::new()),
            accessibility_updates: RefCell::new(VecDeque::new()),
            accessibility_active: Cell::new(false),
            accessibility_focus_provenance: Cell::new(0),
            ready: Cell::new(false),
            animating: Cell::new(false),
            requested_url: RefCell::new(String::new()),
            wake: Arc::new(|| {}),
        }
    }

    fn accessibility_update(
        tree: SurfaceAccessibilityTreeId,
        grafted: Option<SurfaceAccessibilityTreeId>,
    ) -> SurfaceAccessibilityUpdate {
        use servo::accesskit::{Node, NodeId, Role, Tree};
        let mut node = Node::new(Role::GenericContainer);
        if let Some(grafted) = grafted {
            node.set_tree_id(grafted);
        }
        SurfaceAccessibilityUpdate {
            nodes: vec![(NodeId(0), node)],
            tree: Some(Tree::new(NodeId(0))),
            tree_id: tree,
            focus: NodeId(0),
        }
    }

    #[test]
    fn accessibility_callbacks_preserve_order_and_nested_supplier_ids() {
        use servo::accesskit::{NodeId, TreeId, Uuid};
        let delegate = accessibility_delegate();
        let wrapper = TreeId(Uuid::from_u64_pair(0, 101));
        let document = TreeId(Uuid::from_u64_pair(0, 102));
        let iframe = TreeId(Uuid::from_u64_pair(0, 103));
        delegate.accessibility_active.set(true);
        delegate.push_accessibility_update(accessibility_update(wrapper, Some(document)));
        delegate.push_accessibility_update(accessibility_update(document, Some(iframe)));
        delegate.push_accessibility_update(accessibility_update(iframe, None));
        let mut updates = delegate.accessibility_updates.borrow_mut();
        for (tree, grafted) in [
            (wrapper, Some(document)),
            (document, Some(iframe)),
            (iframe, None),
        ] {
            let update = updates.pop_front().unwrap();
            assert_eq!(update.tree_id, tree);
            assert_eq!(update.nodes[0].0, NodeId(0));
            assert_eq!(update.nodes[0].1.tree_id(), grafted);
        }
        assert!(updates.is_empty());
    }

    #[test]
    fn accessibility_deactivation_retires_queued_and_late_callbacks() {
        use servo::accesskit::{TreeId, Uuid};
        let delegate = accessibility_delegate();
        let old = TreeId(Uuid::from_u64_pair(0, 104));
        let replacement = TreeId(Uuid::from_u64_pair(0, 105));
        delegate.accessibility_active.set(true);
        delegate.push_accessibility_update(accessibility_update(old, None));
        delegate.deactivate_accessibility();
        delegate.push_accessibility_update(accessibility_update(old, None));
        assert!(delegate.accessibility_updates.borrow().is_empty());
        delegate.accessibility_active.set(true);
        delegate.push_accessibility_update(accessibility_update(replacement, None));
        let mut updates = delegate.accessibility_updates.borrow_mut();
        assert_eq!(updates.pop_front().unwrap().tree_id, replacement);
        assert!(updates.is_empty());
    }

    #[test]
    fn accessibility_focus_policy_projects_only_focus_after_raw_fifo_admission() {
        use servo::accesskit::{Action, Node, NodeId, Role, Tree, TreeId, Uuid};
        for policy in [A11yFocusPolicy::Upstream, A11yFocusPolicy::DocumentRoot] {
            let delegate = accessibility_delegate();
            delegate.accessibility_active.set(true);
            let raw = [(106, 76), (107, 92)].map(|(id, root_id)| {
                let mut root = Node::new(Role::RootWebArea);
                root.set_children(vec![NodeId(root_id + 1)]);
                let mut child = Node::new(Role::Button);
                child.set_label("supplier node");
                child.add_action(Action::Click);
                SurfaceAccessibilityUpdate {
                    nodes: vec![(NodeId(root_id), root), (NodeId(root_id + 1), child)],
                    tree: Some(Tree::new(NodeId(root_id))),
                    tree_id: TreeId(Uuid::from_u64_pair(0, id)),
                    focus: NodeId(1),
                }
            });
            for update in &raw {
                delegate.push_accessibility_update(update.clone());
            }
            assert_eq!(delegate.accessibility_updates.borrow()[0], raw[0]);
            assert_eq!(delegate.accessibility_updates.borrow()[1], raw[1]);
            for original in raw {
                let mut projected = delegate
                    .poll_projected_accessibility_update(policy)
                    .unwrap();
                assert_eq!(
                    projected.focus,
                    match policy {
                        A11yFocusPolicy::Upstream => original.focus,
                        A11yFocusPolicy::DocumentRoot => original.tree.as_ref().unwrap().root,
                    }
                );
                // Full equality after restoring focus pins IDs, tree metadata,
                // node ordering, children, labels and action masks unchanged.
                projected.focus = original.focus;
                assert_eq!(projected, original);
            }
            assert!(
                delegate
                    .poll_projected_accessibility_update(policy)
                    .is_none()
            );
        }
    }

    #[test]
    fn accessibility_focus_projection_does_not_invent_missing_metadata() {
        use servo::accesskit::{NodeId, TreeId, Uuid};
        let delegate = accessibility_delegate();
        delegate.accessibility_active.set(true);
        let mut raw = accessibility_update(TreeId(Uuid::from_u64_pair(0, 108)), None);
        raw.tree = None;
        raw.focus = NodeId(999);
        delegate.push_accessibility_update(raw.clone());
        assert_eq!(
            delegate
                .poll_projected_accessibility_update(A11yFocusPolicy::DocumentRoot)
                .unwrap(),
            raw
        );
    }

    #[test]
    fn accessibility_focus_setting_is_explicit_and_rejects_unknown_values() {
        assert_eq!(
            parse_a11y_focus_policy(None).unwrap(),
            A11yFocusPolicy::DocumentRoot
        );
        assert_eq!(
            parse_a11y_focus_policy(Some("document-root")).unwrap(),
            A11yFocusPolicy::DocumentRoot
        );
        assert_eq!(
            parse_a11y_focus_policy(Some("upstream")).unwrap(),
            A11yFocusPolicy::Upstream
        );
        for invalid in ["", "root", "Document-Root", "document-root ", "1"] {
            assert!(parse_a11y_focus_policy(Some(invalid)).is_err());
        }
    }

    #[test]
    fn gpu_diagnostics_select_the_requested_wait_and_refuse_unknown_modes() {
        let timeout = std::time::Duration::from_millis(137);
        assert_eq!(
            parse_gpu_sync("existing", timeout).unwrap(),
            DiagnosticGpuSync::Existing
        );
        assert_eq!(
            parse_gpu_sync("producer", timeout).unwrap(),
            DiagnosticGpuSync::ProducerCompletion
        );
        assert_eq!(
            parse_gpu_sync("normalization", timeout).unwrap(),
            DiagnosticGpuSync::NormalizationCompletion { timeout }
        );
        assert_eq!(
            parse_gpu_sync("both", timeout).unwrap(),
            DiagnosticGpuSync::Both { timeout }
        );
        assert!(parse_gpu_sync("unknown", timeout).is_err());
    }

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
