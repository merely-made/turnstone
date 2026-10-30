// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn publish_engine_inventory(&mut self) {
        self.app.engine_inventory =
            project_engine_inventory(&self.content_engines, &self.surface_engines);
    }

    #[cfg(all(feature = "weld", windows))]
    pub(super) fn ensure_weld_engine(&mut self) -> Result<(), String> {
        if self
            .surface_engines
            .contains(inker::routing::ENGINE_WELD_CHROMIUM)
        {
            return Ok(());
        }
        let host = self
            .host
            .as_ref()
            .ok_or_else(|| "the Turnstone wgpu host is not ready".to_string())?;
        let cef_path = std::env::var_os("TURNSTONE_CEF_PATH")
            .or_else(|| std::env::var_os("CEF_PATH"))
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                "set TURNSTONE_CEF_PATH (or CEF_PATH) before selecting weld.chromium".to_string()
            })?;
        let cache_root = self.app.data_root.join("weld").join("cef-cache");
        let runtime = weld::initialize_runtime(cef_path, &cache_root)?;
        let factory =
            weld::TurnstoneWeldFactory::new(runtime, host.device().clone(), host.queue().clone());
        self.surface_engines
            .register(Box::new(weld_engine::WeldEngine::new(Arc::new(factory))));
        self.publish_engine_inventory();
        Ok(())
    }

    #[cfg(not(all(feature = "weld", windows)))]
    pub(super) fn ensure_weld_engine(&mut self) -> Result<(), String> {
        Err("weld.chromium is available only in a Windows build with `--features weld`".into())
    }

    #[cfg(all(feature = "scry", windows))]
    pub(super) fn ensure_scry_engine(&mut self) -> Result<(), String> {
        if self
            .surface_engines
            .contains(inker::routing::ENGINE_SCRYING_WEB)
        {
            return Ok(());
        }
        let host = self
            .host
            .as_ref()
            .ok_or_else(|| "the Turnstone wgpu host is not ready".to_owned())?;
        let factory = Arc::new(scry::TurnstoneScryFactory::new(
            host.device().clone(),
            host.queue().clone(),
        ));
        self.surface_engines
            .register(Box::new(scry::TurnstoneScryEngine::new(factory.clone())));
        self.scry_factory = Some(factory);
        self.publish_engine_inventory();
        Ok(())
    }

    #[cfg(not(all(feature = "scry", windows)))]
    pub(super) fn ensure_scry_engine(&mut self) -> Result<(), String> {
        Err("scrying.web is available only in a Windows build with `--features scry`".into())
    }

    #[cfg(all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    ))]
    pub(super) fn ensure_servo_engine(&mut self) -> Result<std::path::PathBuf, String> {
        if let Some(factory) = &self.servo_factory {
            return Ok(factory.profile_dir().to_owned());
        }
        let host = self
            .host
            .as_ref()
            .ok_or_else(|| "the Turnstone wgpu host is not ready".to_owned())?;
        let profile_name = match std::env::var("TURNSTONE_SERVO_PROFILE") {
            Ok(name) => name,
            Err(std::env::VarError::NotPresent) => "Default".into(),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("TURNSTONE_SERVO_PROFILE must be valid UTF-8".into());
            },
        };
        let profile = servo_profile_directory(
            &self.app.data_root,
            &profile_name,
            std::env::var_os("TURNSTONE_SERVO_PROFILE_DIR").map(std::path::PathBuf::from),
        )?;
        std::fs::create_dir_all(&profile).map_err(|error| {
            format!(
                "could not create Servo profile {}: {error}",
                profile.display()
            )
        })?;
        let profile = effects::browser_profile_path(profile.canonicalize().map_err(|error| {
            format!(
                "could not resolve Servo profile {}: {error}",
                profile.display()
            )
        })?)?;
        let proxy = self.proxy.clone();
        let factory = Arc::new(
            servo::TurnstoneServoFactory::new(
                servo::ServoHostOptions {
                    profile_name,
                    profile_dir: profile.clone(),
                },
                host.device().clone(),
                host.queue().clone(),
                Arc::new(move || {
                    let _ = proxy.send_event(crate::shell::ShellEvent::ActorWake);
                }),
            )
            .map_err(|error| error.to_string())?,
        );
        self.surface_engines
            .register(Box::new(graft_engine::GraftEngine::new(factory.clone())));
        self.servo_factory = Some(factory);
        self.publish_engine_inventory();
        Ok(profile)
    }

    #[cfg(not(all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    )))]
    pub(super) fn ensure_servo_engine(&mut self) -> Result<std::path::PathBuf, String> {
        Err("graft.servo is available only in a Windows, macOS or Linux build with `--features servo`".into())
    }

    /// Poll the value projection after a settings pane persists a write. The
    /// shell owns the redraw boundary and the `ShellChromeConfig`; the pane
    /// remains a retained form with no renderer back-channel.
    pub(super) fn poll_live_settings(&mut self) -> bool {
        self.app
            .apply_chrome_settings_snapshot(&self.live_settings.snapshot())
    }

    /// Lower one app intent through the spine and run what falls out. Syncs
    /// the window's IME enablement to controlled chrome fields on transitions
    /// (a platform call, so it lives here, not in `update`).
    pub(super) fn act(&mut self, action: Action) {
        let _ = self.act_with_command(action, None);
    }

    /// Keyboard commands use the same declaration and reducer as the palette.
    pub(super) fn act_command(&mut self, action: Action) -> Option<cambium::Outcome> {
        let invocation = self.app.command_invocation(&action);
        self.act_with_command(action, invocation)
    }

    pub(super) fn act_with_command(
        &mut self,
        action: Action,
        invocation: Option<cambium::Invocation>,
    ) -> Option<cambium::Outcome> {
        let retention_request =
            if let Action::RetainProducerSelection { choice, selection } = &action {
                let request = uuid::Uuid::new_v4();
                self.latest_clip_retention = Some(super::clip_retaining::RetainStatus {
                    request,
                    choice: choice.clone(),
                    result: None,
                });
                Some((request, choice.clone(), selection.clone()))
            } else {
                None
            };
        let accepted_text = self.accepts_controlled_text();
        let closing = match &action {
            Action::CloseActivePane => self.app.active_pane,
            Action::OuterWorkspace {
                event: workbench::WorkspaceEvent::Tile(workbench::TileEvent::Closed(id)),
                ..
            } => Some(crate::panes::PaneId(id.0)),
            _ => None,
        };
        let (effects, outcome) = match invocation {
            Some(invocation) => self.app.invoke_command(&invocation),
            None => (self.app.update(action), None),
        };
        if let Some((request, choice, selection)) = retention_request {
            let dispatched = effects.iter().any(|effect| matches!(effect,
                Effect::RetainProducerSelection { choice: c, selection: s } if c == &choice && s == &selection));
            if !dispatched {
                if let Some(status) = self
                    .latest_clip_retention
                    .as_mut()
                    .filter(|status| status.request == request)
                {
                    status.record(request, Err("The retained selection was refused".into()));
                }
            }
        }
        if let Some(pane) = closing
            && self.app.space_of(pane).is_none()
        {
            self.evict_pane_renderer(pane);
        }
        let accepts_text = self.accepts_controlled_text();
        if accepts_text != accepted_text
            && let Some(window) = self.window.as_ref()
        {
            window.set_ime_allowed(accepts_text);
        }
        self.run_effects(effects);
        outcome
    }

    pub(super) fn accepts_controlled_text(&self) -> bool {
        if self.outer_focus.is_some()
            && !self.app.omnibar.open
            && !self.app.document_find.open
            && !self.app.user_agent_decision.is_open()
        {
            return false;
        }
        self.pane_text_in_window(0)
            || window_input::document_accepts_ime(&self.content_sessions.pool, self.app.focus, 0)
            || self.app.omnibar.open
            || self.app.document_find.open
            || self.app.user_agent_decision.accepts_text()
    }

    pub(super) fn sync_ime_allowed(&self) {
        if let Some(window) = self.window.as_ref() {
            window.set_ime_allowed(self.accepts_controlled_text());
        }
        for lens in self.lens_windows.values() {
            let window = lens.ordinal as u64 + 1;
            let host_editor = self.app.omnibar.open
                || self.app.document_find.open
                || self.app.user_agent_decision.is_open();
            lens.window.set_ime_allowed(
                !host_editor
                    && (self.pane_text_in_window(window)
                        || window_input::document_accepts_ime(
                            &self.content_sessions.pool,
                            self.app.focus,
                            window,
                        )),
            );
        }
    }

    pub(super) fn inline_text_in_window(&self, window: u64) -> bool {
        if frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            window,
            self.app.session_id.as_uuid(),
        ) {
            return false;
        }
        match self.app.focus {
            crate::surface::FocusTarget::Pane(pane) => {
                self.focus_in_window(window, self.app.focus)
                    && self.renderers.inline_accepts_text(pane)
            },
            _ => false,
        }
    }

    pub(super) fn pane_text_in_window(&self, window: u64) -> bool {
        if frame_keys::header_focus_in_window(
            self.outer_focus.as_ref(),
            window,
            self.app.session_id.as_uuid(),
        ) {
            return false;
        }
        let crate::surface::FocusTarget::Pane(pane) = self.app.focus else {
            return false;
        };
        self.focus_in_window(window, self.app.focus)
            && (self.renderers.inline_accepts_text(pane)
                || self
                    .renderers
                    .contributed
                    .get(pane)
                    .is_some_and(|view| view.accepts_text()))
    }

    pub(super) fn document_ime_cursor_area(&self, window: u64) -> Option<crate::surface::Rect> {
        if self.app.omnibar.open
            || self.app.document_find.open
            || self.app.user_agent_decision.is_open()
            || frame_keys::header_focus_in_window(
                self.outer_focus.as_ref(),
                window,
                self.app.session_id.as_uuid(),
            )
        {
            return None;
        }
        window_input::document_text_input_area(&self.content_sessions.pool, self.app.focus, window)
    }

    pub(super) fn pane_ime_cursor_area(&self, window: u64) -> Option<crate::surface::Rect> {
        if !self.pane_text_in_window(window) {
            return None;
        }
        let crate::surface::FocusTarget::Pane(pane) = self.app.focus else {
            return None;
        };
        let plan = if window == 0 {
            self.surface_plan()
        } else {
            let lens = self
                .lens_windows
                .values()
                .find(|lens| lens.ordinal as u64 + 1 == window)?;
            self.lens_plan(lens.ordinal, lens.width, lens.height)
        };
        let surface = plan
            .iter()
            .find(|s| s.kind == crate::surface::SurfaceKind::Pane(pane))?;
        let (x, y, width, height) = self
            .renderers
            .inline_cursor_area(pane)
            .or_else(|| self.renderers.contributed.get(pane)?.ime_cursor_area())?;
        Some(crate::surface::Rect::new(
            surface.rect.x + x,
            surface.rect.y + y,
            width.max(1.0),
            height.max(1.0),
        ))
    }

    pub(super) fn evict_pane_renderer(&mut self, pane: crate::panes::PaneId) {
        if self.pointer_capture == Some(crate::surface::SurfaceKind::Pane(pane))
            || self.contributed_pointer.is_some_and(|admitted| {
                admitted.surface.kind == crate::surface::SurfaceKind::Pane(pane)
            })
        {
            self.cancel_content_pointer();
        }
        self.renderers.evict(pane);
        let lens_slots = self.app.lenses.len();
        self.content_sessions.retain_appearances(|node, id| {
            !crate::surface::appearance::reader_appearance_belongs_to_pane(
                id, node, pane, lens_slots,
            )
        });
        if self.hovered_pane == Some(pane) {
            self.hovered_pane = None;
        }
    }

    /// Pump documents placed in the primary or any open lens. Lens-only
    /// documents stay visible when the primary renders; an unplaced document
    /// keeps its controller and state without scheduling another redraw.
    pub(super) fn all_content_plans(&self) -> Vec<crate::surface::Surface> {
        let mut plan = self.surface_plan();
        let mut lenses: Vec<_> = self.lens_windows.values().collect();
        lenses.sort_by_key(|lens| lens.ordinal);
        for lens in lenses {
            plan.extend(self.lens_plan(lens.ordinal, lens.width, lens.height));
        }
        plan
    }

    pub(super) fn sync_content_placements(&mut self) {
        use content_pool::HostPlacement;
        self.retain_reading_positions();
        let focus = self.app.focus;
        let color_mode = self
            .app
            .shell_chrome_config()
            .appearance
            .tabard_mode()
            .profile();
        // Surface plans and native pointer positions are physical pixels; the
        // pool places content in layout pixels at its window's scale.
        let placement = |surface: crate::surface::Surface, window, active, scale| {
            let mut host = HostPlacement::for_focus(surface, window, active, focus, scale);
            host.color_mode = color_mode;
            host.reading_preferences = self.app.reading.for_surface(surface, window);
            host.reading_anchor = self
                .app
                .reading
                .anchors
                .iter()
                .find_map(|(target, anchor)| {
                    (target.appearance(window) == surface.id).then(|| anchor.clone())
                });
            host
        };
        let mut placements: Vec<_> = self
            .surface_plan()
            .into_iter()
            .map(|surface| {
                placement(
                    surface,
                    0,
                    self.primary_focused,
                    self.app.shell_chrome_config().layout_scale(0) as f64,
                )
            })
            .collect();
        let mut lenses: Vec<_> = self.lens_windows.values().collect();
        lenses.sort_by_key(|lens| lens.ordinal);
        for lens in lenses {
            placements.extend(
                self.lens_plan(lens.ordinal, lens.width, lens.height)
                    .into_iter()
                    .map(|surface| {
                        placement(
                            surface,
                            lens.ordinal as u64 + 1,
                            lens.focused,
                            self.app
                                .shell_chrome_config()
                                .layout_scale(lens.ordinal + 1) as f64,
                        )
                    }),
            );
        }
        self.content_sessions.sync_placements(
            placements,
            self.content_engines.clone(),
            self.pelt_surface_engines.clone(),
            self.epoch,
        );
        self.cue_frames.retain(&self.content_sessions.pool);
        if let Some(admitted) = self.contributed_pointer
            && !self.contributed_pointer_is_current(admitted)
        {
            self.cancel_content_pointer();
        }
    }

    pub(super) fn pump_visible_documents(&mut self) -> bool {
        self.sync_content_placements();
        let pump_started = std::time::Instant::now();
        let mut visible_pending = false;
        content_pool::pump_document_turn(self, Self::request_pending_document_redraws, |host| {
            for work in host.document_clock.due_work(pump_started) {
                if host
                    .content_sessions
                    .pool
                    .content(&work.content)
                    .and_then(|content| content.session_identity())
                    == Some(work.identity)
                {
                    host.content_sessions.pool.invalidate_paint(&work.content);
                }
            }
            let pending = host.content_sessions.pool.pump();
            // A pump during another window's render can cross a timer's
            // deadline and settle it. Keep that old receiver's final paint
            // before replacing its scheduling observation.
            let due = host.document_clock.due_work(std::time::Instant::now());
            for work in &due {
                if host
                    .content_sessions
                    .pool
                    .content(&work.content)
                    .and_then(|content| content.session_identity())
                    == Some(work.identity)
                {
                    host.content_sessions.pool.invalidate_paint(&work.content);
                }
            }
            host.request_document_work_redraws(&due);
            host.document_clock.observe_work(
                pending,
                host.content_sessions.pool.document_work(),
                pump_started,
            );
            visible_pending = host.content_sessions.pool.visible_needs_frame();
        });
        visible_pending
    }
}
