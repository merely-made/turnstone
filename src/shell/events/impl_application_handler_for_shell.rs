// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ApplicationHandler<crate::shell::ShellEvent> for Shell {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        let started = std::time::Instant::now();
        self.poll_render_recovery();
        self.prune_native_frames();
        self.observe_physics_display_rate(false);
        #[cfg(all(
            feature = "servo",
            any(windows, target_os = "macos", target_os = "linux")
        ))]
        if let Err(error) = super::super::servo::pump() {
            tracing::warn!(%error, "Servo process event pump failed");
        }
        // Native COM calls can consume/coalesce Win32 paint requests during
        // input delivery. A deadline wake requests paint outside WM_PAINT so
        // captured browser mailboxes and self-drive cannot lose their clock.
        // Replace this fallback when the engines expose a host wake callback.
        self.drain_surface_web_events();
        let now = std::time::Instant::now();
        let document_due = self.document_clock.poll_due(now);
        if document_due {
            self.pump_visible_documents();
            self.push_a11y_tree();
        }
        let active = self.content_sessions.surface_count() > 0
            || self.shared_scenario.is_some()
            || self.native_frames.active();
        let surface_due = self.surface_poll_clock.poll_due(active, now);
        if surface_due {
            // The mailbox clock belongs to native producers. Requeuing a
            // quiet component lens every overdue interval can repeatedly
            // service that window before another window receives its paint.
            // Shared state/input/theme invalidation still requests all windows.
            if (self.shared_scenario.is_some()
                || self.content_sessions.pool.window_has_surface(0)
                || self
                    .window
                    .as_ref()
                    .is_some_and(|w| self.native_frames.wants(w.id())))
                && let Some(window) = &self.window
            {
                window.request_redraw();
            }
            for (id, lens) in &self.lens_windows {
                if self.native_frames.wants(*id)
                    || self.pending_lens_capture_window == Some(*id)
                    || self
                        .content_sessions
                        .pool
                        .window_has_surface(lens.ordinal as u64 + 1)
                {
                    lens.window.request_redraw();
                }
            }
        }
        tracing::debug!(target: "turnstone::selfdrive", ?cause, document_due, surface_due,
            micros = started.elapsed().as_micros(), "native wake batch prepared");
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.service_private_custody();
        // Clear before draining: any later proxy callback admits another turn.
        // Semantic publication remains independent of successful presentation.
        if std::mem::take(&mut self.actor_wake_pending) {
            self.drain_actor_updates(event_loop);
        }
        if self.shared_scenario.is_some()
            && self
                .main_receipt
                .presentation_stalled(self.theme_editor.is_some())
        {
            let mut outcome = self.shared_scenario.take().unwrap().finish();
            outcome.ok = false;
            outcome
                .log
                .push("FAIL: no successful window presentation for ten seconds".into());
            self.write_shared_done(&outcome);
            event_loop.exit();
            return;
        }
        let effects = self.app.tick(crate::denizen::now_ms());
        self.run_effects(effects);
        let alembic_idle = self.service_alembic_workshop(event_loop);
        self.pump_redshank();
        self.pump_research();
        #[cfg(all(
            feature = "servo",
            any(windows, target_os = "macos", target_os = "linux")
        ))]
        if let Err(error) = super::super::servo::pump() {
            tracing::warn!(%error, "Servo process event pump failed");
        }
        // Live captured surfaces require a clock independent of paint delivery.
        // Otherwise a minute supplies the idle W4 schedule clock.
        let active = self.content_sessions.surface_count() > 0
            || self.shared_scenario.is_some()
            || self.native_frames.active();
        let now = std::time::Instant::now();
        let deadline = self
            .surface_poll_clock
            .deadline(active, now)
            .into_iter()
            .chain(self.document_clock.deadline(now))
            .min()
            .unwrap_or_else(|| now + std::time::Duration::from_secs(60));
        let editor_idle = self.service_theme_editor(event_loop);
        let editor_idle = if matches!(alembic_idle, cambium_rootstock::IdlePolicy::A11yWake) {
            cambium_rootstock::IdlePolicy::A11yWake
        } else {
            editor_idle
        };
        event_loop.set_control_flow(super::super::appearance_editor::editor_deadline(
            deadline,
            now,
            editor_idle,
        ));
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.cancel_native_frames("host exiting");
        // Retire actual renderer resources while their physical hosts still
        // exist, including a normal scenario exit and a primary-window close.
        let windows: Vec<_> = self
            .lens_windows
            .values()
            .map(|lens| lens.ordinal as u64 + 1)
            .collect();
        for window in windows {
            self.retire_render_receivers(window);
        }
        self.retire_render_receivers(0);
        #[cfg(all(
            feature = "servo",
            any(windows, target_os = "macos", target_os = "linux")
        ))]
        {
            self.clear_surface_content();
            if let Err(error) = super::super::servo::shutdown() {
                tracing::warn!(%error, "Servo process shutdown failed");
            }
        }
        // The trail memory buffers traversals between lifecycle edges, and a
        // normal quit is one: flush-and-release with a bounded ack so the
        // last session's tail survives exit. (The bin needs no exit hook —
        // it persists each record as it arrives.)
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        self.trail_handle
            .command(crate::trail_memory::TrailCommand::Release(ack_tx));
        if ack_rx
            .recv_timeout(std::time::Duration::from_millis(1500))
            .is_err()
        {
            tracing::warn!("trail memory release ack timed out at exit");
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = super::super::titlebar::window_attributes()
            .with_title("Turnstone")
            .with_inner_size(PhysicalSize::new(self.width, self.height))
            // Hidden at creation on purpose: the AccessKit adapter must be
            // installed before the window is first shown, and a window that
            // flashes unbridged is a window a screen reader met too early.
            .with_visible(false);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create the turnstone window"),
        );
        self.publish_titlebar_insets(0, &window);
        self.publish_system_theme(window.theme());
        self.a11y_adapter = Some(crate::shell::a11y_bridge::install(
            &window,
            self.a11y_actions.clone(),
            self.proxy.clone(),
            "Turnstone",
        ));
        window.set_visible(true);
        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);
        self.app.graph_runtimes.resize(self.width, self.height);
        self.app.graph_runtimes.set_physics_display_rate(
            window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz()),
        );
        self.physics_display_rate_last_observed = Some(std::time::Instant::now());
        self.app.viewport = (self.width as f32, self.height as f32);
        self.app.reflow_omnibar();
        // Frame the content, not the origin: a restored session's persisted
        // positions can have settled anywhere in world space, and a camera
        // centered on the origin would then show empty ground.
        self.app.graph_runtimes.fit_to_content();

        let options = NetrenderOptions {
            tile_cache_size: Some(64),
            enable_vello: true,
            // Windows browser producers export D3D12 shared textures. The
            // host must use the same API before constructing either producer.
            #[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
            backends: Some(wgpu::Backends::DX12),
            ..Default::default()
        };
        #[cfg(all(feature = "servo", target_os = "linux"))]
        let booted = super::super::servo::cue::boot_render_core(options).and_then(|core| {
            SurfaceHost::from_shared_core(window.clone(), self.width, self.height, Arc::new(core))
        });
        #[cfg(not(all(feature = "servo", target_os = "linux")))]
        let booted = SurfaceHost::boot(window.clone(), self.width, self.height, options);
        match booted {
            Ok(host) => {
                self.host = Some(host);
                self.ensure_render_recovery();
            },
            Err(err) => {
                eprintln!("[turnstone] {err}");
                event_loop.exit();
                return;
            },
        }

        // Restored Weld-pinned nodes are requested before winit gives us a
        // device. Now that the host exists, replay their ordinary spawn effect
        // through the same registry path a live viewer change uses.
        let pending_surface_spawns = std::mem::take(&mut self.pending_surface_spawns);
        for (node, url) in pending_surface_spawns {
            self.run_effects(vec![crate::action::Effect::SpawnContent { node, url }]);
        }

        // Always-offload physics: the simulation runs on an armillary actor
        // thread and wakes this loop through the proxy when a layout snapshot
        // lands, so a heavy settle never blocks compositing or input.
        let proxy = self.proxy.clone();
        let physics_wake: armillary::Wake = Arc::new(move || {
            let _ = proxy.send_event(crate::shell::ShellEvent::ActorWake);
        });
        self.app.graph_runtimes.offload_physics(physics_wake);

        window.request_redraw();
        self.window = Some(window);
    }

    /// An actor woke us through the proxy: a physics layout snapshot or a
    /// completed fetch is waiting. Drain fetches through the spine, then
    /// redraw so `frame()` folds everything in (and chains while settling).
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: crate::shell::ShellEvent) {
        // Winit's macOS adapter drains proxy callbacks until its queue is empty.
        // Projecting accessibility here can take longer than the producers'
        // cadence, starving window/input events and the next presentation.
        // Keep callbacks cheap; the retained actor queues drain once per batch.
        match event {
            crate::shell::ShellEvent::ActorWake => self.actor_wake_pending = true,
            crate::shell::ShellEvent::NativeFrameReady => self.request_native_frame_redraws(),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        self.service_private_custody();
        if self.theme_editor_event(window_id, &event) {
            return;
        }
        if self.alembic_workshop_event(window_id, &event) {
            return;
        }
        if self.titlebar_event(window_id, &event) {
            return;
        }
        if let (Some(adapter), Some(window)) = (self.a11y_adapter.as_mut(), self.window.as_ref())
            && window.id() == window_id
        {
            tracing::debug!(target: "turnstone::selfdrive", redraw = matches!(event, WindowEvent::RedrawRequested), "begin accessibility event delivery");
            if matches!(
                event,
                WindowEvent::Moved(_)
                    | WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
            ) {
                adapter.update_window_bounds(window);
            }
            if let WindowEvent::Focused(focused) = &event {
                adapter.update_window_focus(*focused);
            }
            tracing::debug!(target: "turnstone::selfdrive", "finished accessibility event delivery");
        }
        if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
            // A lens window's event (rung 7): canvas gestures through the
            // lens's own camera; everything else is the primary's.
            if self.lens_windows.contains_key(&window_id) {
                self.lens_event(event_loop, window_id, event);
                self.drain_pending_windows(event_loop);
            }
            return;
        }
        match event {
            WindowEvent::Focused(focused) => {
                self.primary_focused = focused;
                self.native_input_focus(0, focused);
                self.push_a11y_tree();
            },
            WindowEvent::ThemeChanged(theme) => self.publish_system_theme(Some(theme)),
            WindowEvent::Occluded(false) => {
                // Acquisition can skip an occluded surface without another
                // frame. Repaint on the actual platform visibility transition.
                self.request_redraw();
            },
            WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.observe_physics_display_rate(true);
            },
            WindowEvent::CloseRequested => {
                self.cancel_content_pointer();
                if self.request_close_with_alembic_workshop() {
                    return;
                }
                if self.request_close_with_theme_editor() {
                    return;
                }
                self.act(Action::SaveSession);
                self.release_place_worker();
                event_loop.exit();
            },
            // A dropped file lands at the last tracked cursor position (winit
            // carries no position on the drop event itself; mid-drag hover
            // updates CursorMoved on the platforms that report it).
            WindowEvent::DroppedFile(path) => {
                let (x, y) = self.cursor;
                self.drop_file(x, y, &path);
            },
            WindowEvent::HoveredFile(path) => {
                let (x, y) = self.cursor;
                self.hover_file(x, y, &path);
            },
            WindowEvent::HoveredFileCancelled => self.cancel_host_file_drag(),
            WindowEvent::Resized(size) => {
                self.width = size.width.max(1);
                self.height = size.height.max(1);
                if let Some(host) = self.host.as_mut() {
                    host.resize(self.width, self.height);
                }
                self.app.graph_runtimes.resize(self.width, self.height);
                self.app.viewport = (self.width as f32, self.height as f32);
                self.app.reflow_omnibar();
                self.request_redraw();
            },
            // Continuous gestures map onto the canvas's semantic input methods
            // directly (they are already the right typed vocabulary); Actions
            // are the app-intent tier above. (Architecture plan, the spine.)
            WindowEvent::ModifiersChanged(mods) => {
                self.ctrl = mods.state().control_key();
                self.alt = mods.state().alt_key();
                self.shift = mods.state().shift_key();
                self.meta = mods.state().super_key();
                self.app.graph_runtimes.set_ctrl(mods.state().control_key());
                self.app.graph_runtimes.set_alt(mods.state().alt_key());
            },
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                self.deliver_move(self.cursor.0, self.cursor.1);
                self.deliver_hover(self.cursor.0, self.cursor.1);
                self.update_host_file_drag(self.cursor.0, self.cursor.1);
                let graph_redraw = self
                    .surface_plan()
                    .into_iter()
                    .find(|surface| surface.rect.contains(self.cursor.0, self.cursor.1))
                    .and_then(|surface| match surface.kind {
                        crate::surface::SurfaceKind::Graph(pane) => {
                            Some(self.app.graph_pane_cursor_moved(
                                pane,
                                self.cursor.0 - surface.rect.x,
                                self.cursor.1 - surface.rect.y,
                            ))
                        },
                        _ => None,
                    })
                    .unwrap_or(false);
                if graph_redraw {
                    self.request_redraw();
                }
            },
            WindowEvent::CursorLeft { .. } => self.reset_surface_cursor(),
            WindowEvent::MouseWheel { delta, .. } => {
                // Lines-to-pixels: the canvas pan scale doubles as the content
                // scroll scale (both want ~40px per wheel line).
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * WHEEL_PAN_SCALE, y * WHEEL_PAN_SCALE),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                let (cx, cy) = self.cursor;
                self.deliver_wheel(cx, cy, dx, dy);
            },
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.cursor;
                match state {
                    ElementState::Pressed => self.deliver_press(x, y, button),
                    ElementState::Released => self.deliver_release(x, y, button),
                }
            },
            WindowEvent::Touch(touch) => self.deliver_touch(touch),
            WindowEvent::KeyboardInput { event, .. } => {
                self.deliver_window_key(
                    0,
                    &event.logical_key,
                    event.state == ElementState::Pressed,
                    event.text.as_deref(),
                );
            },
            // IME composition. Preedit is ephemeral by the gesture law — it
            // rides directly on state and only the commit lowers to an
            // Action (`OmnibarInsert`, the same path a future paste takes).
            WindowEvent::Ime(ime) => {
                super::super::controller_input::trace_native_ime(0, &ime);
                if self.app.user_agent_decision.is_open() {
                    if self.app.user_agent_decision.accepts_text()
                        && let Ime::Commit(s) = ime
                    {
                        self.act(Action::InsertAuthentication(s));
                    }
                    return;
                }
                if self.app.document_find.open {
                    self.deliver_find_key(cambium_winit::ime_event_from_winit(&ime));
                    return;
                }
                if !self.app.omnibar.open && self.deliver_window_ime(0, &ime) {
                    self.request_redraw();
                    return;
                }
                if !self.app.omnibar.open {
                    return;
                }
                match ime {
                    Ime::Commit(s) => {
                        self.app.omnibar.preedit = None;
                        self.act(Action::OmnibarInsert(s));
                    },
                    Ime::Preedit(s, _caret) => {
                        self.app.omnibar.preedit = (!s.is_empty()).then_some(s);
                        self.request_redraw();
                    },
                    Ime::Enabled | Ime::Disabled => {},
                }
            },
            WindowEvent::RedrawRequested => {
                self.observe_physics_display_rate(false);
                tracing::debug!(target: "turnstone::selfdrive", "begin render");
                let presented = self.render();
                tracing::debug!(target: "turnstone::selfdrive", "finished render; begin accessibility update");
                self.push_a11y_tree();
                tracing::debug!(target: "turnstone::selfdrive", "finished accessibility update; begin scenario tick");
                if presented {
                    self.scenario_pump(event_loop);
                }
                tracing::debug!(target: "turnstone::selfdrive", "finished scenario tick");
            },
            _ => {},
        }
        self.drain_pending_windows(event_loop);
    }
}
