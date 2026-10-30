// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in super::super) fn ensure_render_recovery(&mut self) {
        if self.render_recovery.is_none() {
            let Some(host) = &self.host else {
                return;
            };
            self.render_recovery = Some(host.shared_core().shared_recovery());
        }
        if self.primary_recovery.is_none() {
            let proxy = self.proxy.clone();
            self.primary_recovery = Some(WindowRecovery::new(
                self.render_recovery.as_ref().expect("shared owner"),
                Arc::new(move || {
                    let _ = proxy.send_event(crate::shell::ShellEvent::ActorWake);
                }),
            ));
        }
    }

    pub(super) fn bind_lens_capture(&mut self) {
        if self.pending_lens_capture.is_none() {
            self.pending_lens_capture_window = None;
        } else if self.pending_lens_capture_window.is_none() {
            self.pending_lens_capture_window = self
                .lens_windows
                .iter()
                .min_by_key(|(_, lens)| lens.ordinal)
                .map(|(id, _)| *id);
        }
    }

    pub(super) fn refuse_window_capture(
        &mut self,
        window: u64,
        refusal: cambium_rootstock::CaptureRefusal,
    ) {
        self.bind_lens_capture();
        let target = self
            .pending_lens_capture_window
            .and_then(|id| self.lens_windows.get(&id))
            .map(|lens| lens.ordinal as u64 + 1);
        let path = take_window_capture(
            window,
            target,
            &mut self.pending_capture,
            &mut self.pending_lens_capture,
        );
        if window != 0 && path.is_some() {
            self.pending_lens_capture_window = None;
        }
        if let Some(path) = path {
            self.main_receipt.errors.push(format!(
                "capture {} window={window}: {refusal}",
                path.display()
            ));
        }
    }

    pub(in super::super) fn refuse_lens_captures_before_clear(&mut self, reason: &str) {
        let windows: Vec<_> = self
            .lens_windows
            .values()
            .map(|lens| lens.ordinal as u64 + 1)
            .collect();
        for window in windows {
            self.retire_frame_focus(window);
            outer_workspace::retire_outer_capture(&mut self.outer_capture, window);
            if let Some(host) = self.host.as_ref() {
                native_composition::retire(
                    host.core(),
                    &mut self.renderers.native_images,
                    &mut self.renderers.native_masks,
                    window as usize,
                );
                if let Some(frame) = self.outer_frames.borrow_mut().get_mut(&(window as usize)) {
                    frame
                        .layout
                        .borrow_mut()
                        .set_native_sources(Default::default());
                }
                host.core()
                    .invalidate_surface(crate::surface::SurfaceId::frame_window(window as usize).0);
            }
        }
        self.outer_frames.borrow_mut().retain(|slot, _| *slot == 0);
        self.bind_lens_capture();
        if let Some(id) = self.pending_lens_capture_window {
            if let Some(lens) = self.lens_windows.get(&id) {
                let window = lens.ordinal as u64 + 1;
                let generation = lens
                    .host
                    .as_ref()
                    .map_or(0, |host| host.core().generation());
                self.refuse_window_capture(
                    window,
                    cambium_rootstock::CaptureRefusal::SurfaceUnavailable {
                        generation,
                        error: reason.into(),
                    },
                );
            }
        }
    }

    /// Retire this physical host's actual admissions without blurring another
    /// window or destroying a retained controller, pane runner or authored draft.
    pub(super) fn retire_render_receivers(&mut self, window: u64) {
        let host = if window == 0 {
            self.host.as_ref()
        } else {
            self.lens_windows
                .values()
                .find(|lens| lens.ordinal as u64 + 1 == window)
                .and_then(|lens| lens.host.as_ref())
                .or(self.host.as_ref())
        };
        if let Some(host) = host {
            native_composition::retire(
                host.core(),
                &mut self.renderers.native_images,
                &mut self.renderers.native_masks,
                window as usize,
            );
            if let Some(frame) = self.outer_frames.borrow_mut().get_mut(&(window as usize)) {
                frame
                    .layout
                    .borrow_mut()
                    .set_native_sources(Default::default());
            }
            host.core()
                .invalidate_surface(crate::surface::SurfaceId::frame_window(window as usize).0);
        }
        self.retire_frame_focus(window);
        outer_workspace::retire_outer_capture(&mut self.outer_capture, window);
        let graph = if window == 0 {
            match self.pointer_capture {
                Some(SurfaceKind::Graph(pane)) => Some(pane),
                _ => None,
            }
        } else {
            self.lens_windows
                .values()
                .find(|lens| lens.ordinal as u64 + 1 == window)
                .and_then(|lens| lens.pointer_graph)
        };
        self.touch_input
            .cancel_window(&mut self.content_sessions.pool, window);
        if self
            .contributed_pointer
            .is_some_and(|held| held.window == window)
        {
            let held = self.contributed_pointer.take().expect("admitted receiver");
            if let SurfaceKind::Pane(pane) = held.surface.kind {
                if let Some(surface) = self.renderers.contributed.get_mut(pane) {
                    surface.mouse_cancel(0., 0.);
                }
            }
            self.pointer_capture = None;
        }
        if let Some(pane) = graph {
            let _ = self
                .app
                .with_graph_pane(pane, |canvas| canvas.pointer_cancel());
        }
        if window == 0 {
            self.pointer_capture = None;
            self.divider_drag = None;
            self.wb_divider_drag = None;
            self.wb_tab_drag = None;
        } else {
            if self
                .lens_divider_drag
                .as_ref()
                .is_some_and(|(ordinal, _)| *ordinal as u64 + 1 == window)
            {
                self.lens_divider_drag = None;
            }
            if let Some(lens) = self
                .lens_windows
                .values_mut()
                .find(|lens| lens.ordinal as u64 + 1 == window)
            {
                lens.pointer_graph = None;
            }
        }
    }

    /// Every subscribed window participates on wakes, including hidden lenses.
    /// The owner alone replaces the device after real resource detachment.
    pub(in super::super) fn poll_render_recovery(&mut self) -> bool {
        self.ensure_render_recovery();
        let Some(owner) = self.render_recovery.clone() else {
            return false;
        };
        if let Some(loss) = owner.status().loss().cloned() {
            self.cancel_native_frames("render device generation lost");
            self.cue_frames.clear();
            let mut can_replace = true;
            #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
            {
                can_replace = ![
                    inker::routing::ENGINE_WELD_CHROMIUM,
                    inker::routing::ENGINE_SCRYING_WEB,
                    inker::routing::ENGINE_GRAFT_SERVO,
                ]
                .iter()
                .any(|engine| self.surface_engines.contains(engine));
                // These caches detach, but registered factories/live native
                // producers still retain Device/Queue. Do not acknowledge them.
                self.surface_frames.clear();
                #[cfg(feature = "scry")]
                self.scry_frame_importers.clear();
            }
            #[cfg(all(feature = "servo", any(target_os = "macos", target_os = "linux")))]
            {
                can_replace &= self.servo_factory.is_none();
            }
            if !can_replace && self.render_recovery_blocked.is_none() {
                let error = "device-loss recovery refused: registered native producers require explicit device rebinding".to_owned();
                tracing::warn!(%error);
                self.main_receipt.errors.push(error.clone());
                self.render_recovery_blocked = Some(error);
            }
            self.refuse_window_capture(
                0,
                cambium_rootstock::CaptureRefusal::DeviceLost(loss.clone()),
            );
            self.retire_render_receivers(0);
            if let Some(mut participation) = self.primary_recovery.take() {
                participation.detach(&owner, loss.generation, can_replace, || {
                    self.host = None;
                });
                self.primary_recovery = Some(participation);
            }
            let windows: Vec<_> = self
                .lens_windows
                .iter()
                .map(|(id, lens)| (*id, lens.ordinal as u64 + 1))
                .collect();
            for (id, window) in windows {
                self.refuse_window_capture(
                    window,
                    cambium_rootstock::CaptureRefusal::DeviceLost(loss.clone()),
                );
                self.retire_render_receivers(window);
                if let Some(lens) = self.lens_windows.get_mut(&id) {
                    lens.recovery
                        .detach(&owner, loss.generation, can_replace, || {
                            lens.host = None;
                        });
                }
            }
            // The installed monitor cancelled handles before Waiting. Collect
            // its original typed terminal once; never replace it with a timeout.
            self.poll_capture_readback();
            if let Some(editor) = self.theme_editor.as_mut() {
                editor.host.wake_turn();
            }
            if !can_replace {
                return false;
            }
        }
        if !admit_recovered_core(owner.recover(), &mut self.main_receipt.errors) {
            return false;
        }
        let Ok(core) = owner.current() else {
            return false;
        };
        let generation = core.generation();
        if self.host.is_none() {
            if let Some((failed, error)) = self
                .primary_recovery
                .as_ref()
                .and_then(|state| state.surface_failure.clone())
            {
                if failed == generation {
                    self.refuse_window_capture(
                        0,
                        cambium_rootstock::CaptureRefusal::SurfaceUnavailable { generation, error },
                    );
                }
            } else if let Some(window) = self.window.clone() {
                match SurfaceHost::from_shared_core(
                    window.clone(),
                    self.width,
                    self.height,
                    core.clone(),
                ) {
                    Ok(host) => {
                        self.host = Some(host);
                        window.request_redraw();
                    },
                    Err(error) => {
                        self.primary_recovery
                            .as_mut()
                            .expect("primary subscription")
                            .surface_failure = Some((generation, error.clone()));
                        self.refuse_window_capture(
                            0,
                            cambium_rootstock::CaptureRefusal::SurfaceUnavailable {
                                generation,
                                error,
                            },
                        );
                    },
                }
            }
        }
        let mut refusals = Vec::new();
        for lens in self.lens_windows.values_mut() {
            if lens.host.is_some() {
                continue;
            }
            if let Some((failed, error)) = &lens.recovery.surface_failure {
                if *failed == generation {
                    refusals.push((lens.ordinal as u64 + 1, error.clone()));
                    continue;
                }
            }
            match SurfaceHost::from_shared_core(
                lens.window.clone(),
                lens.width,
                lens.height,
                core.clone(),
            ) {
                Ok(host) => {
                    lens.host = Some(host);
                    lens.recovery.surface_failure = None;
                    lens.window.request_redraw();
                },
                Err(error) => {
                    lens.recovery.surface_failure = Some((generation, error.clone()));
                    refusals.push((lens.ordinal as u64 + 1, error));
                },
            }
        }
        drop(core);
        for (window, error) in refusals {
            self.refuse_window_capture(
                window,
                cambium_rootstock::CaptureRefusal::SurfaceUnavailable { generation, error },
            );
        }
        true
    }

    pub(super) fn refuse_raster_frame(
        &mut self,
        window: u64,
        generation: u64,
        source_key: u64,
        failure: genet_render_host::RasterizeFailure,
    ) {
        self.cancel_native_window(window, "raster frame refused");
        let failure = if matches!(
            &failure,
            genet_render_host::RasterizeFailure::ProviderError(_)
        ) {
            self.render_recovery
                .as_ref()
                .and_then(|owner| owner.status().loss().cloned())
                .map(genet_render_host::RasterizeFailure::DeviceLost)
                .unwrap_or(failure)
        } else {
            failure
        };
        let loss = matches!(&failure, genet_render_host::RasterizeFailure::DeviceLost(_));
        let refusal = raster_capture_refusal(failure, generation, source_key);
        let error = format!("window={window} surface={source_key}: {refusal}");
        if !self.main_receipt.errors.contains(&error) {
            self.main_receipt.errors.push(error);
        }
        self.refuse_window_capture(window, refusal);
        if loss {
            self.poll_render_recovery();
        }
    }

    pub(super) fn detach_render_surface(&mut self, window: u64) {
        self.cancel_native_frames("render surface detached");
        let generation = if window == 0 {
            self.host
                .as_ref()
                .map_or(0, |host| host.core().generation())
        } else {
            self.lens_windows
                .values()
                .find(|lens| lens.ordinal as u64 + 1 == window)
                .and_then(|lens| lens.host.as_ref())
                .map_or(0, |host| host.core().generation())
        };
        // A capture admitted to this surface cannot silently become a capture
        // of its replacement. The caller may explicitly arm a fresh request.
        self.refuse_window_capture(
            window,
            cambium_rootstock::CaptureRefusal::SurfaceUnavailable {
                generation,
                error: "surface lost before its requested presentation".into(),
            },
        );
        self.retire_render_receivers(window);
        if window == 0 {
            self.host = None;
        } else if let Some(lens) = self
            .lens_windows
            .values_mut()
            .find(|lens| lens.ordinal as u64 + 1 == window)
        {
            lens.host = None;
        }
        self.poll_render_recovery();
    }
}
