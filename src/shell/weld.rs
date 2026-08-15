//! Turnstone's Windows `weld.chromium` host implementation.
//!
//! The inker adapter stays CEF-free. This module owns the process runtime,
//! per-tile CEF producer, and the vocabulary translations for the deliberately
//! small first projection.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use inker::{
    CursorShape, DataTransfer, DataTransferItem, DragEvent, DragOperationSet, DragPhase,
    FocusReason, FrameHandleOwnership, KeyboardEvent, KeyboardModifiers, MouseButton, MouseEvent,
    MouseEventKind, NativeTextureHandle, NavigationEvent, PhysicalPosition, PointerButtons,
    PointerEvent, PointerPhase, PointerType, SurfaceError, SurfaceSettings, SurfaceSyncHandle,
    SurfaceTextureFormat, WebFeatureStatus, WebFrameTransportMode, WebMessage,
    WebSurfaceCapabilities, WebSurfaceEvent,
};
use weld_engine::{WeldFrame, WeldProducerFactory, WeldSurface};
use welding::{
    CefRuntime, CefRuntimeConfig, CefSurfaceConfig, CefSurfaceProducer, FocusDirection,
    HostWgpuContext, KeyEvent, KeyEventKind, MouseAction, PlatformCefConfig as WindowsCefConfig,
    PlatformCefProducer as WindowsCefProducer,
};
use winit::dpi::PhysicalSize;

pub(super) struct TurnstoneWeldFactory {
    runtime: Arc<CefRuntime>,
    host: HostWgpuContext,
}

impl TurnstoneWeldFactory {
    pub(super) fn new(runtime: Arc<CefRuntime>, device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            runtime,
            host: HostWgpuContext::new(device, queue),
        }
    }
}

impl WeldProducerFactory for TurnstoneWeldFactory {
    fn build(
        &self,
        request: &inker::SurfaceSpawnRequest,
    ) -> Result<Box<dyn WeldSurface>, SurfaceError> {
        let mut surface = CefSurfaceConfig::default();
        surface.initial_url = request.url.clone();
        surface.initial_size = PhysicalSize::new(request.width.max(1), request.height.max(1));
        let profile_dir = PathBuf::from(&request.profile.user_data_dir);
        // CEF creates a profile's files but not a missing parent chain. Make
        // the resolved per-node directory real before request-context creation
        // so a failed mkdir cannot silently fall back to another profile.
        std::fs::create_dir_all(&profile_dir).map_err(|error| {
            SurfaceError::SpawnFailed(format!(
                "could not create Weld profile {}: {error}",
                profile_dir.display()
            ))
        })?;
        surface.user_data_dir = Some(profile_dir);
        let mut producer = WindowsCefProducer::new(
            self.runtime.as_ref(),
            WindowsCefConfig { surface },
            &self.host,
        )
        .map_err(weld_spawn_error)?;
        producer.set_visible(true).map_err(weld_spawn_error)?;
        Ok(Box::new(TurnstoneWeldSurface { producer }))
    }
}

pub(super) fn initialize_runtime(
    cef_path: PathBuf,
    cache_root: &Path,
) -> Result<Arc<CefRuntime>, String> {
    std::fs::create_dir_all(cache_root).map_err(|error| {
        format!(
            "could not create Weld cache root {}: {error}",
            cache_root.display()
        )
    })?;
    let mut config = CefRuntimeConfig::new(cef_path);
    config.cache_path = Some(cache_root.to_path_buf());
    config.user_agent = std::env::var("TURNSTONE_WELD_USER_AGENT")
        .ok()
        .filter(|value| !value.is_empty());
    config.user_agent_product = std::env::var("TURNSTONE_WELD_USER_AGENT_PRODUCT")
        .ok()
        .filter(|value| !value.is_empty());
    if config.user_agent.is_some() && config.user_agent_product.is_some() {
        return Err(
            "set only TURNSTONE_WELD_USER_AGENT or TURNSTONE_WELD_USER_AGENT_PRODUCT, not both"
                .into(),
        );
    }
    CefRuntime::initialize(config)
        .map(Arc::new)
        .map_err(|error| format!("could not initialize Weld CEF runtime: {error}"))
}

struct TurnstoneWeldSurface {
    producer: WindowsCefProducer,
}

impl WeldSurface for TurnstoneWeldSurface {
    fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.producer
            .resize(PhysicalSize::new(width.max(1), height.max(1)))
            .map_err(weld_input_error)
    }

    fn acquire_frame(&mut self) -> Result<Option<WeldFrame>, SurfaceError> {
        let Some(frame) = self.producer.acquire_native_frame() else {
            return Ok(None);
        };
        let size = frame.size;
        let format = map_texture_format(frame.format)?;
        let resource_epoch = frame.generation;
        let handle = frame.into_raw_handle() as u64;
        Ok(Some(WeldFrame {
            texture: NativeTextureHandle::D3d12Shared {
                handle,
                ownership: FrameHandleOwnership::Transferred,
            },
            sync: SurfaceSyncHandle::None,
            width: size.width,
            height: size.height,
            format,
            resource_epoch,
        }))
    }

    fn load_url(&mut self, url: &str) -> Result<(), SurfaceError> {
        self.producer.navigate_to_url(url).map_err(weld_input_error)
    }

    fn load_html(&mut self, html: &str) -> Result<(), SurfaceError> {
        self.producer
            .navigate_to_string(html, "text/html")
            .map_err(weld_input_error)
    }

    fn reload(&mut self) -> Result<(), SurfaceError> {
        self.producer.reload().map_err(weld_input_error)
    }

    fn stop(&mut self) -> Result<(), SurfaceError> {
        self.producer.stop().map_err(weld_input_error)
    }

    fn go_back(&mut self) -> Result<(), SurfaceError> {
        self.producer.go_back().map_err(weld_input_error)
    }

    fn go_forward(&mut self) -> Result<(), SurfaceError> {
        self.producer.go_forward().map_err(weld_input_error)
    }

    fn can_go_back(&self) -> bool {
        self.producer.can_go_back()
    }

    fn can_go_forward(&self) -> bool {
        self.producer.can_go_forward()
    }

    fn notify_mouse(&mut self, event: MouseEvent) -> Result<(), SurfaceError> {
        self.producer
            .send_mouse_input(welding::MouseEvent {
                x: event.position.x.round() as i32,
                y: event.position.y.round() as i32,
                button: map_mouse_button(event.button)?,
                action: map_mouse_action(event.kind),
                modifiers: Default::default(),
            })
            .map_err(weld_input_error)
    }

    fn notify_pointer(&mut self, event: PointerEvent) -> Result<(), SurfaceError> {
        match event.pointer_type {
            PointerType::Mouse => self
                .producer
                .send_mouse_input(welding::MouseEvent {
                    x: event.position.x.round() as i32,
                    y: event.position.y.round() as i32,
                    button: map_mouse_button(event.button)?,
                    action: match event.phase {
                        PointerPhase::Down => MouseAction::Pressed,
                        PointerPhase::Move => MouseAction::Moved,
                        PointerPhase::Up | PointerPhase::Cancel => MouseAction::Released,
                    },
                    modifiers: map_event_modifiers(event.modifiers, event.buttons),
                })
                .map_err(weld_input_error),
            PointerType::Pen | PointerType::Touch => {
                if event.pointer_id < 0 {
                    return Err(SurfaceError::InputFailed(
                        "CEF touch contact ids must be non-negative".into(),
                    ));
                }
                let active = matches!(event.phase, PointerPhase::Down | PointerPhase::Move);
                self.producer
                    .send_touch_input(welding::TouchInput {
                        id: event.pointer_id,
                        device: match event.pointer_type {
                            PointerType::Pen => welding::ContactDevice::Pen,
                            PointerType::Touch => welding::ContactDevice::Touch,
                            _ => unreachable!(),
                        },
                        x: event.position.x,
                        y: event.position.y,
                        radius_x: (event.width / 2.0).max(0.0),
                        radius_y: (event.height / 2.0).max(0.0),
                        rotation_angle: event.twist.unwrap_or(0.0),
                        pressure: event.pressure.unwrap_or(if active { 0.5 } else { 0.0 }),
                        phase: match event.phase {
                            PointerPhase::Down => welding::TouchPhase::Started,
                            PointerPhase::Move => welding::TouchPhase::Moved,
                            PointerPhase::Up => welding::TouchPhase::Ended,
                            PointerPhase::Cancel => welding::TouchPhase::Cancelled,
                        },
                        modifiers: map_event_modifiers(event.modifiers, event.buttons),
                    })
                    .map_err(weld_input_error)
            }
            PointerType::Unknown => Err(SurfaceError::Unsupported(
                "Weld cannot truthfully choose a CEF pointer type for an unknown device".into(),
            )),
        }
    }

    fn notify_drag(&mut self, event: DragEvent) -> Result<(), SurfaceError> {
        let payload = matches!(event.phase, DragPhase::Enter)
            .then(|| map_data_transfer(&event.data_transfer))
            .transpose()?;
        let allowed_operations = map_drag_operations(event.data_transfer.allowed_operations);
        self.producer
            .send_drag_input(welding::DragInput {
                kind: match event.phase {
                    DragPhase::Enter => welding::DragEventKind::Enter,
                    DragPhase::Over => welding::DragEventKind::Over,
                    DragPhase::Leave => welding::DragEventKind::Leave,
                    DragPhase::Drop => welding::DragEventKind::Drop,
                },
                payload,
                x: event.position.x.round() as i32,
                y: event.position.y.round() as i32,
                modifiers: map_event_modifiers(event.modifiers, event.buttons),
                allowed_operations,
            })
            .map_err(weld_input_error)
    }

    fn finish_drag_source(
        &mut self,
        position: PhysicalPosition,
        operation: DragOperationSet,
    ) -> Result<(), SurfaceError> {
        self.producer
            .finish_drag_source(
                position.x.round() as i32,
                position.y.round() as i32,
                map_drag_operations(operation),
            )
            .map_err(weld_input_error)
    }

    fn notify_keyboard(&mut self, event: KeyboardEvent) -> Result<(), SurfaceError> {
        let modifiers = welding::EventModifiers {
            shift: event.modifiers.shift,
            ctrl: event.modifiers.ctrl,
            alt: event.modifiers.alt,
            meta: event.modifiers.meta,
            ..Default::default()
        };
        let key = KeyEvent {
            kind: if event.pressed {
                KeyEventKind::RawKeyDown
            } else {
                KeyEventKind::KeyUp
            },
            windows_key_code: event.key_code as i32,
            native_key_code: event.scan_code as i32,
            character: None,
            modifiers,
        };
        self.producer
            .send_keyboard_input(key)
            .map_err(weld_input_error)?;
        if event.pressed
            && let Some(character) = event.text.and_then(|text| text.chars().next())
        {
            self.producer
                .send_keyboard_input(KeyEvent {
                    kind: KeyEventKind::Char,
                    windows_key_code: event.key_code as i32,
                    native_key_code: event.scan_code as i32,
                    character: Some(character),
                    modifiers,
                })
                .map_err(weld_input_error)?;
        }
        Ok(())
    }

    fn focus(&mut self, reason: FocusReason) -> Result<(), SurfaceError> {
        let direction = match reason {
            FocusReason::ShiftTab => FocusDirection::Backward,
            FocusReason::Mouse | FocusReason::Tab | FocusReason::Programmatic => {
                FocusDirection::Forward
            }
        };
        self.producer
            .move_focus(direction)
            .map_err(weld_input_error)
    }

    fn poll_navigation_event(&mut self) -> Option<NavigationEvent> {
        self.producer
            .poll_navigation_event()
            .and_then(map_navigation_event)
    }

    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.producer.poll_cursor_shape().map(map_cursor_shape)
    }

    fn poll_web_message(&mut self) -> Option<WebMessage> {
        self.producer.poll_web_message().map(|payload| WebMessage {
            tag: "weld".into(),
            payload,
        })
    }

    fn poll_web_event(&mut self) -> Option<WebSurfaceEvent> {
        if let Some(event) = self.producer.poll_navigation_event() {
            return Some(map_weld_web_event(event));
        }
        self.poll_web_message().map(WebSurfaceEvent::WebMessage)
    }

    fn web_capabilities(&self) -> WebSurfaceCapabilities {
        let mut capabilities = WebSurfaceCapabilities {
            backend_name: "weld.cef.windows".into(),
            backend_version: None,
            frame_transport: WebFrameTransportMode::ImportedTexture,
            ..Default::default()
        };
        capabilities.devtools = WebFeatureStatus::Supported;
        capabilities.pointer.mouse = WebFeatureStatus::Supported;
        capabilities.pointer.pen = WebFeatureStatus::Partial {
            detail: "CEF accepts pen contacts, but winit 0.30 does not identify pen versus touch"
                .into(),
        };
        capabilities.pointer.touch = WebFeatureStatus::Supported;
        capabilities.pointer.contact_geometry = WebFeatureStatus::Partial {
            detail:
                "the contract and CEF carry contact geometry; winit touch events do not supply it"
                    .into(),
        };
        capabilities.pointer.pressure = WebFeatureStatus::Supported;
        capabilities.pointer.tangential_pressure = WebFeatureStatus::unsupported(
            "CEF's touch-event input has no tangential-pressure field",
        );
        capabilities.pointer.tilt =
            WebFeatureStatus::unsupported("CEF's touch-event input has no tilt fields");
        capabilities.pointer.twist = WebFeatureStatus::Partial {
            detail: "the contract and CEF carry twist; winit touch events do not supply it".into(),
        };
        capabilities.pointer.altitude_azimuth =
            WebFeatureStatus::unsupported("CEF's touch-event input has no altitude/azimuth fields");
        capabilities.drag_drop.host_to_page = WebFeatureStatus::Supported;
        capabilities.drag_drop.page_to_host = WebFeatureStatus::Partial {
            detail:
                "Weld reports page drags, but Turnstone cannot start a native winit drag loop yet"
                    .into(),
        };
        capabilities.drag_drop.file_items = WebFeatureStatus::Supported;
        capabilities.drag_drop.string_items = WebFeatureStatus::Partial {
            detail: "text/plain, text/html, and text/uri-list are projected; arbitrary MIME strings are rejected"
                .into(),
        };
        capabilities.degradation_reasons = vec![
            "Turnstone projects pointer input and host-to-page drag/drop; page-to-host drag is observable but has no native winit drag loop".into(),
            "PDF, native printing, downloads, cookies, script results, CDP, permissions, auth, popup composition, and snapshots have no Turnstone control surface yet".into(),
        ];
        capabilities
    }

    fn apply_settings(&mut self, settings: &SurfaceSettings) -> Result<(), SurfaceError> {
        if settings.dev_tools {
            self.producer.open_devtools().map_err(weld_input_error)?;
        }
        if (settings.zoom_factor - 1.0).abs() > f64::EPSILON {
            return Err(SurfaceError::Unsupported(
                "Turnstone's Weld tile has no absolute zoom mapping yet".into(),
            ));
        }
        Ok(())
    }

    fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError> {
        Err(SurfaceError::Unsupported(
            "Turnstone has not projected Weld snapshots yet".into(),
        ))
    }
}

fn map_event_modifiers(
    modifiers: KeyboardModifiers,
    buttons: PointerButtons,
) -> welding::EventModifiers {
    welding::EventModifiers {
        shift: modifiers.shift,
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        meta: modifiers.meta,
        left_mouse_button: buttons.contains(PointerButtons::PRIMARY),
        middle_mouse_button: buttons.contains(PointerButtons::AUXILIARY),
        right_mouse_button: buttons.contains(PointerButtons::SECONDARY),
    }
}

fn map_drag_operations(operations: DragOperationSet) -> welding::DragOperations {
    let mut mapped = welding::DragOperations::NONE;
    if operations.contains(DragOperationSet::COPY) {
        mapped = mapped | welding::DragOperations::COPY;
    }
    if operations.contains(DragOperationSet::LINK) {
        mapped = mapped | welding::DragOperations::LINK;
    }
    if operations.contains(DragOperationSet::MOVE) {
        mapped = mapped | welding::DragOperations::MOVE;
    }
    mapped
}

fn map_weld_drag_operations(operations: welding::DragOperations) -> DragOperationSet {
    let mut mapped = DragOperationSet::NONE;
    if operations.0 & welding::DragOperations::COPY.0 != 0 {
        mapped = mapped | DragOperationSet::COPY;
    }
    if operations.0 & welding::DragOperations::LINK.0 != 0 {
        mapped = mapped | DragOperationSet::LINK;
    }
    if operations.0 & welding::DragOperations::MOVE.0 != 0 {
        mapped = mapped | DragOperationSet::MOVE;
    }
    mapped
}

fn map_data_transfer(transfer: &DataTransfer) -> Result<welding::DragPayload, SurfaceError> {
    let mut payload = welding::DragPayload::default();
    for item in &transfer.items {
        match item {
            DataTransferItem::File {
                path, display_name, ..
            } => payload.files.push(welding::DragFile {
                path: path.clone(),
                display_name: display_name.clone(),
            }),
            DataTransferItem::String { mime_type, data } => {
                match mime_type.to_ascii_lowercase().as_str() {
                    "text/plain" => payload.fragment_text = Some(data.clone()),
                    "text/html" => payload.fragment_html = Some(data.clone()),
                    "text/uri-list" => {
                        payload.link_url = data
                            .lines()
                            .map(str::trim)
                            .find(|line| !line.is_empty() && !line.starts_with('#'))
                            .map(str::to_owned);
                    }
                    other => {
                        return Err(SurfaceError::Unsupported(format!(
                            "Weld cannot preserve dragged string MIME type {other}"
                        )));
                    }
                }
            }
        }
    }
    Ok(payload)
}

fn map_weld_drag_payload(
    payload: welding::DragPayload,
    operations: welding::DragOperations,
) -> DataTransfer {
    let mut items = Vec::new();
    items.extend(
        payload
            .files
            .into_iter()
            .map(|file| DataTransferItem::File {
                mime_type: String::new(),
                path: file.path,
                display_name: file.display_name,
            }),
    );
    if let Some(data) = payload.link_url {
        items.push(DataTransferItem::String {
            mime_type: "text/uri-list".into(),
            data,
        });
    }
    if let Some(data) = payload.fragment_text {
        items.push(DataTransferItem::String {
            mime_type: "text/plain".into(),
            data,
        });
    }
    if let Some(data) = payload.fragment_html {
        items.push(DataTransferItem::String {
            mime_type: "text/html".into(),
            data,
        });
    }
    DataTransfer {
        items,
        allowed_operations: map_weld_drag_operations(operations),
    }
}

fn map_texture_format(format: wgpu::TextureFormat) -> Result<SurfaceTextureFormat, SurfaceError> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => Ok(SurfaceTextureFormat::Rgba8Unorm),
        wgpu::TextureFormat::Rgba8UnormSrgb => Ok(SurfaceTextureFormat::Rgba8UnormSrgb),
        wgpu::TextureFormat::Bgra8Unorm => Ok(SurfaceTextureFormat::Bgra8Unorm),
        wgpu::TextureFormat::Bgra8UnormSrgb => Ok(SurfaceTextureFormat::Bgra8UnormSrgb),
        other => Err(SurfaceError::Unsupported(format!(
            "Weld emitted an unmapped texture format {other:?}"
        ))),
    }
}

fn map_mouse_button(button: Option<MouseButton>) -> Result<welding::MouseButton, SurfaceError> {
    match button.unwrap_or(MouseButton::Left) {
        MouseButton::Left => Ok(welding::MouseButton::Left),
        MouseButton::Middle => Ok(welding::MouseButton::Middle),
        MouseButton::Right => Ok(welding::MouseButton::Right),
        MouseButton::Back | MouseButton::Forward => Err(SurfaceError::Unsupported(
            "CEF mouse back/forward buttons are not projected by Turnstone yet".into(),
        )),
    }
}

fn map_mouse_action(kind: MouseEventKind) -> MouseAction {
    match kind {
        MouseEventKind::Moved => MouseAction::Moved,
        MouseEventKind::Pressed => MouseAction::Pressed,
        MouseEventKind::Released => MouseAction::Released,
        MouseEventKind::ScrollPixels { delta_x, delta_y }
        | MouseEventKind::ScrollLines { delta_x, delta_y } => MouseAction::WheelScrolled {
            delta_x: delta_x.round() as i32,
            delta_y: delta_y.round() as i32,
        },
    }
}

fn map_navigation_event(event: welding::NavigationEvent) -> Option<NavigationEvent> {
    match event {
        welding::NavigationEvent::LoadStart { url } => Some(NavigationEvent::Started { url }),
        welding::NavigationEvent::LoadEnd { url, .. } => {
            Some(NavigationEvent::Finished { url, title: None })
        }
        welding::NavigationEvent::LoadError {
            url, error_text, ..
        } => Some(NavigationEvent::Failed {
            url,
            reason: error_text,
        }),
        welding::NavigationEvent::AddressChanged { url } => {
            Some(NavigationEvent::Committed { url })
        }
        _ => None,
    }
}

fn map_weld_web_event(event: welding::NavigationEvent) -> WebSurfaceEvent {
    match event {
        welding::NavigationEvent::LoadStart { url } => {
            WebSurfaceEvent::Navigation(NavigationEvent::Started { url })
        }
        welding::NavigationEvent::LoadEnd { url, .. } => {
            WebSurfaceEvent::Navigation(NavigationEvent::Finished { url, title: None })
        }
        welding::NavigationEvent::LoadError {
            url, error_text, ..
        } => WebSurfaceEvent::Navigation(NavigationEvent::Failed {
            url,
            reason: error_text,
        }),
        welding::NavigationEvent::AddressChanged { url } => {
            WebSurfaceEvent::Navigation(NavigationEvent::Committed { url })
        }
        welding::NavigationEvent::TitleChanged { title } => WebSurfaceEvent::TitleChanged { title },
        welding::NavigationEvent::ContentProcessTerminated {
            status,
            error_code,
            error_string,
        } => WebSurfaceEvent::ProcessCrashed {
            reason: format!("{status:?} ({error_code}): {error_string}"),
        },
        welding::NavigationEvent::NewWindowRequested { url, user_gesture } => {
            if !user_gesture {
                tracing::debug!(%url, "Weld auxiliary navigable lacked user activation");
            }
            WebSurfaceEvent::NewWindowRequested { url }
        }
        welding::NavigationEvent::ContextMenuRequested {
            x,
            y,
            link_url,
            source_url,
            ..
        } => WebSurfaceEvent::ContextMenuRequested {
            x: x.into(),
            y: y.into(),
            link_url: (!link_url.is_empty()).then_some(link_url),
            image_url: (!source_url.is_empty()).then_some(source_url),
        },
        welding::NavigationEvent::PermissionRequested {
            origin,
            permissions,
            ..
        } => WebSurfaceEvent::PermissionRequested {
            kind: permissions
                .into_iter()
                .map(|kind| format!("{kind:?}"))
                .collect::<Vec<_>>()
                .join(","),
            origin,
        },
        welding::NavigationEvent::AuthChallenged {
            origin_url, realm, ..
        } => WebSurfaceEvent::AuthRequested {
            origin: origin_url,
            realm: (!realm.is_empty()).then_some(realm),
        },
        welding::NavigationEvent::DownloadStarted {
            url,
            suggested_filename,
            ..
        } => WebSurfaceEvent::DownloadRequested {
            url,
            suggested_name: (!suggested_filename.is_empty()).then_some(suggested_filename),
        },
        welding::NavigationEvent::DragStarted {
            payload,
            allowed_operations,
            x,
            y,
        } => WebSurfaceEvent::PageDragStarted {
            data_transfer: map_weld_drag_payload(payload, allowed_operations),
            position: PhysicalPosition {
                x: x as f32,
                y: y as f32,
            },
        },
        welding::NavigationEvent::ConsoleMessage {
            level,
            message,
            source,
            line,
        } => WebSurfaceEvent::ConsoleMessage {
            level: level.to_string(),
            text: message,
            source: (!source.is_empty()).then_some(source),
            line: u32::try_from(line).ok(),
        },
        other => WebSurfaceEvent::BackendDiagnostic {
            severity: "debug".into(),
            message: format!("unprojected Weld event: {other:?}"),
        },
    }
}

fn map_cursor_shape(shape: welding::CursorShape) -> CursorShape {
    match shape {
        welding::CursorShape::Default => CursorShape::Default,
        welding::CursorShape::Pointer => CursorShape::Pointer,
        welding::CursorShape::Text => CursorShape::Text,
        welding::CursorShape::Crosshair => CursorShape::Crosshair,
        welding::CursorShape::Move | welding::CursorShape::ResizeAll => CursorShape::Move,
        welding::CursorShape::NotAllowed => CursorShape::NotAllowed,
        welding::CursorShape::ResizeNs => CursorShape::ResizeNs,
        welding::CursorShape::ResizeEw => CursorShape::ResizeEw,
        welding::CursorShape::ResizeNeSw => CursorShape::ResizeNesw,
        welding::CursorShape::ResizeNwSe => CursorShape::ResizeNwse,
        welding::CursorShape::Grab => CursorShape::Grab,
        welding::CursorShape::Grabbing => CursorShape::Grabbing,
        welding::CursorShape::Custom(value) if value == "none" => CursorShape::Hidden,
        _ => CursorShape::Default,
    }
}

fn weld_spawn_error(error: welding::WeldError) -> SurfaceError {
    SurfaceError::SpawnFailed(error.to_string())
}

fn weld_input_error(error: welding::WeldError) -> SurfaceError {
    SurfaceError::InputFailed(error.to_string())
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn data_transfer_preserves_files_and_standard_string_types() {
        let transfer = DataTransfer {
            items: vec![
                DataTransferItem::File {
                    mime_type: "text/plain".into(),
                    path: PathBuf::from("receipt.txt"),
                    display_name: Some("receipt.txt".into()),
                },
                DataTransferItem::String {
                    mime_type: "text/uri-list".into(),
                    data: "# source\nhttps://example.com/receipt".into(),
                },
                DataTransferItem::String {
                    mime_type: "text/plain".into(),
                    data: "Receipt".into(),
                },
                DataTransferItem::String {
                    mime_type: "text/html".into(),
                    data: "<b>Receipt</b>".into(),
                },
            ],
            allowed_operations: DragOperationSet::COPY | DragOperationSet::LINK,
        };
        let payload = map_data_transfer(&transfer).unwrap();
        assert_eq!(payload.files.len(), 1);
        assert_eq!(
            payload.link_url.as_deref(),
            Some("https://example.com/receipt")
        );
        assert_eq!(payload.fragment_text.as_deref(), Some("Receipt"));
        assert_eq!(payload.fragment_html.as_deref(), Some("<b>Receipt</b>"));
    }

    #[test]
    fn arbitrary_drag_string_mime_is_rejected_instead_of_dropped() {
        let transfer = DataTransfer {
            items: vec![DataTransferItem::String {
                mime_type: "application/vnd.example.receipt".into(),
                data: "opaque".into(),
            }],
            allowed_operations: DragOperationSet::COPY,
        };
        assert!(matches!(
            map_data_transfer(&transfer),
            Err(SurfaceError::Unsupported(_))
        ));
    }

    #[test]
    fn page_drag_effects_round_trip_without_backend_only_bits() {
        let native = welding::DragOperations::COPY
            | welding::DragOperations::MOVE
            | welding::DragOperations::PRIVATE;
        let mapped = map_weld_drag_operations(native);
        assert!(mapped.contains(DragOperationSet::COPY));
        assert!(mapped.contains(DragOperationSet::MOVE));
        assert!(!mapped.contains(DragOperationSet::LINK));
    }
}
