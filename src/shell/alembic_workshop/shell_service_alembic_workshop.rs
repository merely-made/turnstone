// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(in super::super) fn service_alembic_workshop(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> IdlePolicy {
        if self.alembic_workshop_requested {
            self.alembic_workshop_requested = false;
            if let Some(window) = &self.alembic_workshop {
                if let Some(native) = window.host.native_window() {
                    native.focus_window();
                }
            } else if let Some(core) = self.host.as_ref().map(|host| host.shared_core()) {
                let result = (|| {
                    let graph = self.app.graph_runtimes.active_graph();
                    let owner = crate::alembic_workshop::owner(
                        "turnstone-public-example",
                        *graph.as_uuid(),
                        *self.app.session_id.as_uuid(),
                    );
                    let human = self
                        .app
                        .personae_root()
                        .ok_or_else(|| "Unlock the profile before opening Alembic.".to_string())?;
                    let protection = self
                        .custody_binding
                        .as_ref()
                        .map(|binding| binding.protection.clone())
                        .ok_or_else(|| "Unlock the profile before opening Alembic.".to_string())?;
                    let directory = self.app.session_dir().join("alembic-workshop");
                    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                    let backend = DirectoryBackend::open(&directory).map_err(|e| e.to_string())?;
                    let install = installation(&owner);
                    let subject = subject(&owner);
                    let mut state = SurfaceState::new();
                    state.choices = self.alembic_workshop_choices(&owner);
                    let wake_proxy = self.proxy.clone();
                    let wake: armillary::Wake = Arc::new(move || {
                        let _ = wake_proxy.send_event(crate::shell::ShellEvent::ActorWake);
                    });
                    let host = WinitHost::with_shared_render_core(
                        HostOptions {
                            title: "Turnstone — Evidence brief workshop".into(),
                            initial_logical_size: (900.0, 760.0),
                            window_frame: WindowFrame::App,
                            ..Default::default()
                        },
                        move |_, commands, _| native_init(state, commands),
                        native_hooks(),
                        core,
                        wake.clone(),
                    );
                    let mut window = WorkshopWindow {
                        host,
                        driver: Some(Driver::new_sealed(
                            backend,
                            owner.clone(),
                            install,
                            wake,
                            protection,
                        )),
                        owner,
                        installation: install,
                        human,
                        subject,
                    };
                    window.host.open(event_loop)?;
                    Ok::<_, String>(window)
                })();
                match result {
                    Ok(window) => {
                        self.alembic_workshop = Some(window);
                        self.open_recover_alembic_workshop();
                        let load = self.app.begin_saved_source_load();
                        self.run_effects(vec![load]);
                        self.refresh_alembic_workshop_sources();
                    },
                    Err(error) => {
                        tracing::warn!(%error, "could not open Alembic workshop");
                    },
                }
            }
        }

        let mut idle = IdlePolicy::Wait;
        let mut redraw_after_poll = false;
        let actions = if let Some(window) = self.alembic_workshop.as_mut() {
            window.host.wake_turn();
            idle = window.host.idle_turn();
            let mut actions = Vec::new();
            if let Some(runner) = window.host.s.runner.as_mut() {
                runner.update(|state| actions = state.take_actions());
            }
            actions
        } else {
            Vec::new()
        };
        for action in actions {
            self.handle_alembic_action(action);
        }
        if let Some(window) = self.alembic_workshop.as_mut() {
            let owner = window.owner.clone();
            let installation = window.installation;
            let human = window.human;
            let run = window
                .state()
                .and_then(|s| s.view.as_ref())
                .and_then(|v| v.run);
            let polled = if let Some(driver) = window.driver.as_mut() {
                if driver.busy() {
                    let inputs = run
                        .and_then(|id| pollster::block_on(driver.workshop.run_inputs(id)).ok())
                        .unwrap_or_default();
                    let authority = crate::alembic_workshop::authority(
                        &owner,
                        installation,
                        human,
                        &inputs,
                        crate::denizen::now_ms(),
                    );
                    let now = || crate::denizen::now_ms();
                    let protection = self
                        .custody_binding
                        .as_ref()
                        .map(|binding| binding.protection.clone());
                    let source_gate = |passage: &alembic::workshop::Passage| {
                        workshop_passage_current(&self.app, &owner, protection.as_deref(), passage)
                    };
                    let result = pollster::block_on(driver.poll(&authority, &now, &source_gate));
                    Some((result, driver.busy()))
                } else {
                    None
                }
            } else {
                None
            };
            if let Some((result, busy)) = polled {
                match result {
                    Ok(Some(view)) => window.update(|state| {
                        state.view = Some(view);
                        state.busy = busy;
                    }),
                    Ok(None) => {},
                    Err(error) => window.status("The helper run stopped.", Some(error.to_string())),
                }
                redraw_after_poll = true;
                idle = IdlePolicy::A11yWake;
            }
        }
        if redraw_after_poll {
            self.request_redraw();
        }
        if self
            .alembic_workshop
            .as_ref()
            .is_some_and(|w| w.host.s.close_requested)
        {
            if let Some(mut window) = self.alembic_workshop.take() {
                let run = window
                    .state()
                    .and_then(|s| s.view.as_ref())
                    .and_then(|v| v.run);
                if let Some(driver) = window.driver.as_mut() {
                    let inputs = run
                        .and_then(|id| pollster::block_on(driver.workshop.run_inputs(id)).ok())
                        .unwrap_or_default();
                    let authority = crate::alembic_workshop::authority(
                        &window.owner,
                        window.installation,
                        window.human,
                        &inputs,
                        crate::denizen::now_ms(),
                    );
                    let now = || crate::denizen::now_ms();
                    if let Err(error) = pollster::block_on(driver.shutdown(&authority, &now)) {
                        tracing::error!(%error, "Alembic shutdown did not confirm persistence");
                    }
                }
            }
            if self.close_after_alembic_workshop {
                self.close_after_alembic_workshop = false;
                self.act(crate::action::Action::SaveSession);
                self.release_place_worker();
                event_loop.exit();
            } else if let Some(native) = &self.window {
                native.focus_window();
                native.request_redraw();
            }
            idle = IdlePolicy::Wait;
        }
        idle
    }

    pub(super) fn alembic_workshop_choices(
        &self,
        owner: &alembic::workshop::Owner,
    ) -> Vec<SourceChoice> {
        if owner.session != self.app.session_id.as_uuid().to_string()
            || owner.workspace != self.app.graph_runtimes.active_graph().as_uuid().to_string()
        {
            return Vec::new();
        }
        let mut choices = crate::alembic_workshop::example_choices(self.app.graph_runtimes.graph());
        let current_admissions = self.app.saved_source_admissions();
        let custody_open = self
            .custody_binding
            .as_ref()
            .is_some_and(|binding| binding.protection.epoch().is_some());
        let mut private = crate::alembic_workshop::private_source_choices(
            owner,
            &self.app.saved_sources.entries,
            &current_admissions,
            custody_open,
        );
        if private.is_empty() {
            let (version, refusal) = match self.app.saved_sources.status.as_str() {
                "Loading" => (
                    "loading",
                    "Loading sealed saved sources attached to this graph.".to_string(),
                ),
                "Ready" => (
                    "empty",
                    "No saved sources are attached to this graph yet.".to_string(),
                ),
                status if status.starts_with("Unavailable:") => (
                    "unavailable",
                    "Saved source versions could not be loaded. Try searching again.".to_string(),
                ),
                _ => (
                    "pending",
                    "Saved source versions will load when the workshop opens.".to_string(),
                ),
            };
            private.push(SourceChoice {
                key: format!("private-sources-{version}"),
                title: "Sealed saved sources".into(),
                version: version.into(),
                selected: false,
                available: false,
                refusal: Some(refusal),
            });
        }
        choices.extend(private);
        choices
    }

    pub(in super::super) fn refresh_alembic_workshop_sources(&mut self) {
        let Some(owner) = self
            .alembic_workshop
            .as_ref()
            .map(|window| window.owner.clone())
        else {
            return;
        };
        let choices = self.alembic_workshop_choices(&owner);
        let source_status = self.app.saved_sources.status.clone();
        if let Some(window) = self.alembic_workshop.as_mut() {
            window.update(|state| {
                let mut update = state.clone();
                update.choices = choices;
                if !state.busy && state.view.is_none() {
                    match source_status.as_str() {
                        "Loading" => {
                            update.status = "Loading sealed saved sources.".into();
                            update.error = None;
                        },
                        "Ready" => {
                            update.status =
                                "Saved source choices are current for this graph.".into();
                            update.error = None;
                        },
                        status if status.starts_with("Unavailable:") => {
                            update.status = "Saved source choices are unavailable.".into();
                            update.error = Some(status.to_owned());
                        },
                        _ => {},
                    }
                }
                state.apply_owner_update(update);
            });
        }
        self.request_redraw();
    }

    pub(super) fn open_recover_alembic_workshop(&mut self) {
        let Some(window) = self.alembic_workshop.as_mut() else {
            return;
        };
        let owner = window.owner.clone();
        let installation = window.installation;
        let human = window.human;
        let subject = window.subject;
        let recovered = if let Some(driver) = window.driver.as_mut() {
            match pollster::block_on(driver.open(subject)) {
                Ok(view) => {
                    let run = view.run;
                    let inputs = run
                        .and_then(|id| pollster::block_on(driver.workshop.run_inputs(id)).ok())
                        .unwrap_or_default();
                    let authority = crate::alembic_workshop::authority(
                        &owner,
                        installation,
                        human,
                        &inputs,
                        crate::denizen::now_ms(),
                    );
                    let now = || crate::denizen::now_ms();
                    pollster::block_on(driver.recover(&authority, &now)).map_err(|error| {
                        (
                            "Prior helper state could not be recovered.",
                            error.to_string(),
                        )
                    })
                },
                Err(error) => Err((
                    "Could not load the retained helper state.",
                    error.to_string(),
                )),
            }
        } else {
            return;
        };
        match recovered {
            Ok(view) => window.update(|state| state.view = Some(view)),
            Err((message, error)) => window.status(message, Some(error)),
        }
        self.request_redraw();
    }
}
