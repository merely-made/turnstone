// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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
            Some(tree) => Ok(Some(SurfaceAccessibilityTreeId(tree.0))),
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
    #[cfg(target_os = "linux")]
    fn acquire_frame(&mut self) -> Result<Option<GraftFrame>, SurfaceError> {
        self.root.servo.spin_event_loop();
        let interop = self.interop.as_ref().unwrap();
        let mut native = interop
            .try_acquire_frame()
            .map_err(|e| SurfaceError::FrameAcquisitionFailed(e.to_string()))?;
        if native.is_none() {
            if interop.has_pending_frame() {
                return Ok(None);
            }
            if !self.delegate.ready.replace(false) && !self.delegate.animating.get() {
                return Ok(None);
            }
            self.view().paint();
            interop.rendering_context().present();
            native = interop
                .try_acquire_frame()
                .map_err(|e| SurfaceError::FrameAcquisitionFailed(e.to_string()))?;
        }
        let Some(frame) = native else {
            return Ok(None);
        };
        let metadata = frame.metadata();
        self.frame_epoch = metadata.generation;
        Ok(Some(GraftFrame {
            texture: NativeTextureHandle::OwnedPayload(Box::new(ServoDmaBufFrame { frame })),
            sync: SurfaceSyncHandle::None,
            width: metadata.size.width,
            height: metadata.size.height,
            format: SurfaceTextureFormat::Rgba8Unorm,
            resource_epoch: metadata.generation,
        }))
    }
    #[cfg(not(target_os = "linux"))]
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
            // Upstream AAC exposes leave/out, but those events do not clear
            // pressed mouse buttons or dispatch pointercancel. A synthetic
            // release could activate a control, so report the actual limit.
            PointerPhase::Cancel => return Err(unsupported("mouse cancellation")),
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
    fn set_focus(&mut self, focused: bool) -> Result<(), SurfaceError> {
        self.view().set_focused(focused);
        Ok(())
    }
    fn poll_navigation_event(&mut self) -> Option<NavigationEvent> {
        None
    }
    fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
        self.delegate.cursors.borrow_mut().pop_front()
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
    fn set_page_zoom(&mut self, factor: f32) -> Result<inker::DocumentZoomState, SurfaceError> {
        if !factor.is_finite() || factor <= 0. {
            return Err(unsupported("non-finite or nonpositive page zoom"));
        }
        self.view().set_page_zoom(factor);
        Ok(inker::DocumentZoomState {
            requested: factor,
            applied: self.view().page_zoom(),
            min: 0.1,
            max: 10.,
        })
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
