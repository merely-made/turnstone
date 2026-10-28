// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's window and owner service for the shared Alembic surface.

use super::Shell;
use alembic::workshop::{Control, Destination, Owner, Review};
use alembic::workshop_native::{NativeHost, native_hooks, native_init};
use alembic::workshop_surface::{RetainedReading, SourceChoice, SurfaceAction, SurfaceState};
use athanor::workshop_driver::Driver;
use cambium_genet_winit_host::{CloseRequest, HostOptions, WindowFrame, WinitHost};
use cambium_rootstock::IdlePolicy;
use muniment::DirectoryBackend;
use std::sync::Arc;
use winit::{event::WindowEvent, event_loop::ActiveEventLoop, window::WindowId};

pub(super) struct WorkshopWindow {
    pub host: NativeHost,
    driver: Option<Driver<DirectoryBackend>>,
    owner: Owner,
    installation: [u8; 16],
    human: [u8; 32],
    subject: [u8; 32],
}

fn subject(owner: &Owner) -> [u8; 32] {
    *blake3::hash(format!("turnstone-public-example-helper-v1/{owner:?}").as_bytes()).as_bytes()
}

fn installation(owner: &Owner) -> [u8; 16] {
    let digest = blake3::hash(format!("turnstone-alembic-install-v1/{owner:?}").as_bytes());
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id
}

impl WorkshopWindow {
    fn state(&self) -> Option<&SurfaceState> {
        self.host.s.runner.as_ref().map(|runner| runner.state())
    }
    fn update(&mut self, f: impl FnOnce(&mut SurfaceState)) {
        if let Some(runner) = self.host.s.runner.as_mut() {
            runner.update(f);
        }
    }
    fn status(&mut self, status: impl Into<String>, error: Option<String>) {
        let status = status.into();
        self.update(|state| {
            state.status = status;
            state.error = error;
        });
    }
}

impl Shell {
    pub(super) fn service_alembic_workshop(&mut self, event_loop: &ActiveEventLoop) -> IdlePolicy {
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
                    state.choices =
                        crate::alembic_workshop::example_choices(self.app.graph_runtimes.graph());
                    state.choices.extend(self.app.saved_sources.entries.iter().enumerate().map(|(index, _entry)| SourceChoice {
                        key: format!("private-not-admitted:{index}"),
                        title: "Private saved source".into(), version: String::new(),
                        selected: false, available: false,
                        refusal: Some("Private saved sources are not admitted by this public example workflow.".into()),
                    }));
                    let wake_proxy = self.proxy.clone();
                    let wake: armillary::Wake = Arc::new(move || {
                        let _ = wake_proxy.send_event(());
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
                    Ok(window) => self.alembic_workshop = Some(window),
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
                    let source_gate = |passage: &alembic::workshop::Passage| {
                        crate::alembic_workshop::passage_current(&self.app, &owner, passage)
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

    fn handle_alembic_action(&mut self, action: SurfaceAction) {
        match action {
            SurfaceAction::LoadExample => self.load_alembic_examples(),
            SurfaceAction::Run => self.run_alembic(),
            SurfaceAction::Control(control) => self.control_alembic(control),
            SurfaceAction::OpenSource { proposal, source } => {
                let reading = if let Some(window) = self.alembic_workshop.as_mut() {
                    let owner = window.owner.clone();
                    let graph_id = self.app.graph_runtimes.active_graph();
                    if owner.session != self.app.session_id.as_uuid().to_string()
                        || owner.workspace != graph_id.as_uuid().to_string()
                    {
                        Err(
                            "The workshop owner changed; reopen it before opening a citation."
                                .into(),
                        )
                    } else if let Some(driver) = window.driver.as_ref() {
                        match pollster::block_on(driver.refresh()) {
                            Ok(view) => crate::alembic_workshop::proposal_reading(
                                &owner, &view, proposal, &source,
                            ),
                            Err(error) => {
                                Err(format!("The saved proposal could not be reopened: {error}"))
                            },
                        }
                    } else {
                        Err("The saved proposal is unavailable.".into())
                    }
                } else {
                    Err("The evidence brief workshop is closed.".into())
                };
                match reading {
                    Ok(reading) => {
                        if let Some(window) = self.alembic_workshop.as_mut() {
                            window.update(|state| state.reading = Some(reading));
                        }
                    },
                    Err(error) => self.status_alembic(
                        "The cited retained passage could not be opened.",
                        Some(error),
                    ),
                }
            },
            SurfaceAction::OpenResult(resource) => {
                let result = self.alembic_workshop.as_ref().and_then(|window| {
                    if window.owner.session != self.app.session_id.as_uuid().to_string()
                        || window.owner.workspace
                            != self.app.graph_runtimes.active_graph().as_uuid().to_string()
                    {
                        return Some(Err(
                            "The workshop owner changed; reopen it for the current session and graph."
                                .to_owned(),
                        ));
                    }
                    let Some(runtime) = self.app.graph_runtimes.get(
                        self.app.graph_runtimes.active_graph(),
                    ) else {
                        return Some(Err("The destination graph is unavailable.".to_owned()));
                    };
                    let protection = self
                        .custody_binding
                        .as_ref()
                        .map(|binding| binding.protection.clone());
                    let Some(protection) = protection else {
                        return Some(Err("Unlock the profile to open this retained brief.".to_owned()));
                    };
                    let Some(read_epoch) = protection.epoch() else {
                        return Some(Err("Profile custody is unavailable.".to_owned()));
                    };
                    match athanor::workshop_sealed_destination::open_result_resource(
                        runtime.canvas.graph(),
                        &window.owner,
                        &window.owner.workspace,
                        &resource,
                        protection.as_ref(),
                    ) {
                        Ok((_, text)) if protection.epoch() == Some(read_epoch) => {
                            Some(Ok((true, text)))
                        },
                        Ok(_) => Some(Err(
                            "Profile custody changed while opening the retained brief.".into(),
                        )),
                        Err(athanor::workshop::Error::Missing) => {
                            let resource_id = uuid::Uuid::parse_str(&resource).ok();
                            let address = resource_id.and_then(|expected| {
                                runtime.canvas.graph().nodes().find_map(|(key, node)| {
                                    (runtime.canvas.graph().shown_resource_id(key) == Some(expected)
                                        && node.url().starts_with(
                                            "https://mere.computer/notes/evidence-brief/",
                                        ))
                                    .then(|| node.url().to_owned())
                                })
                            });
                            address
                                .map(|address| Ok((false, address)))
                                .or_else(|| {
                                    Some(Err("No public or sealed retained brief matches this Resource.".into()))
                                })
                        },
                        Err(error) => Some(Err(error.to_string())),
                    }
                });
                match result {
                    Some(Ok((private, value))) => {
                        if private {
                            if let Some(window) = self.alembic_workshop.as_mut() {
                                window.update(|state| {
                                    state.reading = Some(RetainedReading {
                                        title: "Private evidence brief".into(),
                                        version: resource.clone(),
                                        text: value,
                                    });
                                });
                            }
                            self.status_alembic("Opened the retained evidence brief.", None);
                        } else {
                            self.act(crate::action::Action::OpenAddress(value));
                            self.status_alembic(
                                "Opened the retained public evidence brief in the owner graph.",
                                None,
                            );
                        }
                    },
                    Some(Err(error)) => self
                        .status_alembic("Could not open the retained result.", Some(error)),
                    None => self.status_alembic(
                        "No retained evidence brief was selected.",
                        Some("The workshop did not provide a result resource.".into()),
                    ),
                }
            },
        }
    }

    fn status_alembic(&mut self, status: impl Into<String>, error: Option<String>) {
        if let Some(window) = self.alembic_workshop.as_mut() {
            window.status(status, error);
        }
    }

    fn load_alembic_examples(&mut self) {
        let Some(graph_id) = self
            .alembic_workshop
            .as_ref()
            .map(|w| self.app.graph_runtimes.active_graph())
        else {
            return;
        };
        if self.alembic_workshop.as_ref().is_some_and(|w| {
            w.owner.session != self.app.session_id.as_uuid().to_string()
                || w.owner.workspace != graph_id.as_uuid().to_string()
        }) {
            self.status_alembic(
                "The current session or graph changed.",
                Some("Reopen the workshop for the current owner.".into()),
            );
            return;
        }
        let directory = self.app.session_dir();
        let Some(runtime) = self.app.graph_runtimes.get(graph_id) else {
            return;
        };
        let mut candidate = runtime.canvas.graph().clone();
        if crate::alembic_workshop::example_seed_conflict(&candidate) {
            self.status_alembic(
                "The reserved public example IDs already belong to other graph content.",
                Some("No example content was changed.".into()),
            );
            return;
        }
        candidate.set_recorder(None);
        for delta in crate::alembic_workshop::example_seed_deltas(&candidate) {
            if let mere::kernel::graph::apply::GraphDeltaResult::Refused(error) =
                mere::kernel::graph::apply::apply_graph_delta(&mut candidate, delta)
            {
                self.status_alembic(
                    "The public example could not be staged safely.",
                    Some(error.to_string()),
                );
                return;
            }
        }
        let choices = crate::alembic_workshop::example_choices(&candidate);
        if choices.iter().any(|choice| !choice.available) {
            self.status_alembic(
                "The public example corpus could not be staged completely.",
                Some("No graph snapshot was written.".into()),
            );
            return;
        }
        let session = self.app.session_id;
        let path = directory.join(pandect::session_graph_store::GRAPH_FILE);
        let result = runtime.persistence.save_graph_with(
            &directory,
            &candidate,
            || pandect::save_node_facets(&directory, candidate.facets()),
            |placement| crate::session_graph_file::save_profiled(&path, &candidate, placement),
        );
        if let Err(error) = result {
            self.status_alembic(
                "Could not retain the public example corpus.",
                Some(error.to_string()),
            );
            return;
        }
        let Some(runtime) = self.app.graph_runtimes.get_mut(graph_id) else {
            return;
        };
        runtime.canvas.set_graph(candidate);
        if let Err(error) = self.app.bind_behavior_journal() {
            self.app.refuse_behavior(error);
        }
        let choices = crate::alembic_workshop::example_choices(self.app.graph_runtimes.graph());
        let saved_private = self
            .app
            .saved_sources
            .entries
            .iter()
            .enumerate()
            .map(|(index, _entry)| SourceChoice {
                key: format!("private-not-admitted:{index}"),
                title: "Private saved source".into(),
                version: String::new(),
                selected: false,
                available: false,
                refusal: Some(
                    "Private saved sources are not admitted by this public example workflow."
                        .into(),
                ),
            })
            .collect::<Vec<_>>();
        self.status_alembic(
            format!("Public examples retained in session {}.", session.as_uuid()),
            None,
        );
        if let Some(window) = self.alembic_workshop.as_mut() {
            window.update(|state| {
                state.choices = choices.into_iter().chain(saved_private).collect();
                state.busy = false;
            });
            let owner = window.owner.clone();
            let installation = window.installation;
            let human = window.human;
            let subject = window.subject;
            let recovered = if let Some(driver) = window.driver.as_mut() {
                match pollster::block_on(driver.open(subject)) {
                    Ok(_) => {
                        let current = pollster::block_on(driver.refresh()).ok();
                        let inputs = current
                            .and_then(|view| view.run)
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
        }
        self.request_redraw();
    }

    fn run_alembic(&mut self) {
        let Some(window) = self.alembic_workshop.as_mut() else {
            return;
        };
        let Some(selected) = window.state().map(|state| state.selected()) else {
            return;
        };
        let owner = window.owner.clone();
        let installation = window.installation;
        let human = window.human;
        let Some(graph) = self
            .app
            .graph_runtimes
            .get(self.app.graph_runtimes.active_graph())
            .map(|r| r.canvas.graph())
        else {
            return;
        };
        let passages =
            match crate::alembic_workshop::capture_public_examples(graph, &owner, &selected) {
                Ok(passages) => passages,
                Err(error) => {
                    window.status("Choose two to four retained public examples.", Some(error));
                    return;
                },
            };
        let authority = crate::alembic_workshop::authority(
            &owner,
            installation,
            human,
            &passages,
            crate::denizen::now_ms(),
        );
        let now = || crate::denizen::now_ms();
        let source_gate = |passage: &alembic::workshop::Passage| {
            crate::alembic_workshop::passage_current(&self.app, &owner, passage)
        };
        let (started, busy) = if let Some(driver) = window.driver.as_mut() {
            let started =
                pollster::block_on(driver.begin(passages, &authority, &now, &source_gate));
            (started, driver.busy())
        } else {
            return;
        };
        match started {
            Ok(view) => window.update(|state| {
                state.view = Some(view);
                state.busy = busy;
                state.status = "Generating from the selected public retained passages.".into();
                state.error = None;
            }),
            Err(error) => window.status(
                "The selected sources were refused or could not start.",
                Some(error.to_string()),
            ),
        }
    }

    fn control_alembic(&mut self, control: Control) {
        if control == Control::Accept || control == Control::Reconcile {
            self.accept_or_reconcile_alembic(control == Control::Reconcile);
            return;
        }
        let Some(window) = self.alembic_workshop.as_mut() else {
            return;
        };
        let view = match window.state().and_then(|s| s.view.clone()) {
            Some(view) => view,
            None => return,
        };
        let owner = window.owner.clone();
        let installation = window.installation;
        let human = window.human;
        let now = || crate::denizen::now_ms();
        let outcome = {
            let Some(driver) = window.driver.as_mut() else {
                return;
            };
            if matches!(
                control,
                Control::Pause | Control::Resume | Control::Revoke | Control::Dissolve
            ) {
                let inputs = view
                    .run
                    .and_then(|id| pollster::block_on(driver.workshop.run_inputs(id)).ok())
                    .unwrap_or_default();
                let authority = crate::alembic_workshop::authority(
                    &owner,
                    installation,
                    human,
                    &inputs,
                    crate::denizen::now_ms(),
                );
                let gate = |p: &alembic::workshop::Passage| {
                    crate::alembic_workshop::passage_current(&self.app, &owner, p)
                };
                pollster::block_on(driver.control(control, &authority, &now, &gate))
                    .map(|view| Some((view, driver.busy())))
                    .map_err(|error| ("Lifecycle change was refused.", error.to_string()))
            } else if control == Control::Reject {
                if let Some(run) = view.run {
                    match pollster::block_on(driver.workshop.reject(run)) {
                        Ok(()) => pollster::block_on(driver.refresh())
                            .map(|view| Some((view, false)))
                            .map_err(|error| ("Could not reload review state.", error.to_string())),
                        Err(error) => Err(("Review rejection failed.", error.to_string())),
                    }
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        };
        match outcome {
            Ok(Some((view, busy))) => window.update(|state| {
                state.view = Some(view);
                state.busy = busy;
            }),
            Ok(None) => {},
            Err((message, error)) => window.status(message, Some(error)),
        }
    }

    fn accept_or_reconcile_alembic(&mut self, reconcile: bool) {
        let Some(window) = self.alembic_workshop.as_mut() else {
            return;
        };
        let view = match window.state().and_then(|s| s.view.clone()) {
            Some(view) => view,
            None => return,
        };
        let Some(run) = view.run else {
            return;
        };
        let graph_id = self.app.graph_runtimes.active_graph();
        let owner = window.owner.clone();
        if owner.session != self.app.session_id.as_uuid().to_string()
            || owner.workspace != graph_id.as_uuid().to_string()
        {
            window.status(
                "The destination owner changed.",
                Some("Reopen the workshop for the current session and graph.".into()),
            );
            return;
        }
        let human = window.human;
        let installation = window.installation;
        let Some(protection) = self
            .custody_binding
            .as_ref()
            .map(|binding| binding.protection.clone())
        else {
            window.status(
                "Unlock the profile before accepting this brief.",
                Some("Private owner protection is unavailable.".into()),
            );
            return;
        };
        let now_ms = crate::denizen::now_ms();
        let directory = self.app.session_dir();
        let live_graph = match self.app.graph_runtimes.get(graph_id) {
            Some(r) => r.canvas.graph(),
            None => {
                window.status(
                    "Destination graph unavailable.",
                    Some("The current graph owner is unavailable.".into()),
                );
                return;
            },
        };
        let disk_graph = match crate::session::try_load_session_graph_locked(&directory) {
            Ok(Some(loaded)) => Some(loaded.graph),
            Ok(None) => None,
            Err(error) => {
                window.status(
                    "Destination graph could not be read for recovery.",
                    Some(error),
                );
                return;
            },
        };
        if reconcile && disk_graph.is_none() {
            window.status(
                "The prior destination write cannot be proven absent.",
                Some("No complete durable graph snapshot is available for reconciliation.".into()),
            );
            return;
        }
        let authoritative_before = disk_graph.unwrap_or_else(|| live_graph.clone());
        if !reconcile {
            let live_revision = match athanor::workshop_destination::graph_revision(live_graph) {
                Ok(revision) => revision,
                Err(error) => {
                    window.status(
                        "Live graph cannot be fingerprinted.",
                        Some(error.to_string()),
                    );
                    return;
                },
            };
            let disk_revision = match athanor::workshop_destination::graph_revision(&authoritative_before) {
                Ok(revision) => revision,
                Err(error) => {
                    window.status(
                        "Saved graph cannot be fingerprinted.",
                        Some(error.to_string()),
                    );
                    return;
                },
            };
            if live_revision != disk_revision {
                window.status(
                    "The saved graph changed outside this owner.",
                    Some("Reload the current session before accepting.".into()),
                );
                return;
            }
        }
        let mut candidate = authoritative_before.clone();
        candidate.set_recorder(None);
        let revision = match athanor::workshop_destination::graph_revision(&candidate) {
            Ok(revision) => revision,
            Err(error) => {
                window.status(
                    "Destination graph cannot be fingerprinted.",
                    Some(error.to_string()),
                );
                return;
            },
        };
        let dest = Destination {
            owner: owner.clone(),
            graph: graph_id.as_uuid().to_string(),
            revision,
        };
        let authority = crate::alembic_workshop::authority_for_destination(
            &owner,
            installation,
            human,
            &dest,
            &view
                .proposal
                .as_ref()
                .map(|proposal| {
                    proposal
                        .sources
                        .iter()
                        .map(|passage| passage.source.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            now_ms,
        );
        let result = {
            let Some(driver) = window.driver.as_mut() else {
                return;
            };
            pollster::block_on(async {
                let proposal = view
                    .proposal
                    .as_ref()
                    .ok_or_else(|| "No proposal is awaiting review.".to_string())?;
                let acceptance = if reconcile {
                    match &view.review {
                        Some(
                            alembic::workshop::Review::Applying(a)
                            | alembic::workshop::Review::Reconciling(a),
                        ) => a.clone(),
                        _ => {
                            return Err(
                                "No interrupted destination operation is awaiting reconciliation."
                                    .into(),
                            );
                        },
                    }
                } else {
                    if proposal.sources.iter().any(|passage| {
                        !crate::alembic_workshop::passage_current(&self.app, &owner, passage)
                    }) {
                        return Err(
                            "A cited public source version changed; review or generate again."
                                .into(),
                        );
                    }
                    driver
                        .workshop
                        .accept(run, dest.clone(), human, &authority, now_ms)
                        .await
                        .map_err(|e| e.to_string())?
                };
                let private_destination = proposal.sources.iter().any(|passage| !passage.public);
                if reconcile {
                    let existing = if private_destination {
                        athanor::workshop_sealed_destination::query_graph(
                            &candidate,
                            &owner,
                            &dest.graph,
                            &acceptance,
                            proposal,
                            protection.as_ref(),
                        )
                    } else {
                        athanor::workshop_destination::query_graph(
                            &candidate,
                            &owner,
                            &dest.graph,
                            &acceptance,
                            proposal,
                        )
                    }
                    .map_err(|e| e.to_string())?;
                    if let Some(receipt) = existing {
                        let epoch = protection.epoch().ok_or_else(|| {
                            "Profile custody is unavailable.".to_string()
                        })?;
                        driver
                            .workshop
                            .acknowledge(run, receipt.clone())
                            .await
                            .map_err(|e| e.to_string())?;
                        return Ok((receipt, false, Some(epoch)));
                    }
                }
                if proposal.sources.iter().any(|passage| {
                    !crate::alembic_workshop::passage_current(&self.app, &owner, passage)
                }) {
                    return Err(
                        "A cited public source version changed; the destination was not reapplied."
                            .into(),
                    );
                }
                let prepared = if private_destination {
                    match athanor::workshop_sealed_destination::prepare_graph(
                        &candidate,
                        &owner,
                        &dest.graph,
                        &acceptance,
                        proposal,
                        human,
                        &authority,
                        now_ms,
                        protection.as_ref(),
                    )
                    .map_err(|e| e.to_string())?
                    {
                        athanor::workshop_sealed_destination::Preparation::Existing {
                            receipt,
                            epoch,
                        } => (None, receipt, Some(epoch)),
                        athanor::workshop_sealed_destination::Preparation::New {
                            deltas,
                            receipt,
                            epoch,
                        } => (Some(deltas), receipt, Some(epoch)),
                    }
                } else {
                    let revision = athanor::workshop_destination::graph_revision(&candidate)
                        .map_err(|e| e.to_string())?;
                    match athanor::workshop_destination::prepare_graph(
                        &candidate,
                        &owner,
                        &dest.graph,
                        &revision,
                        &acceptance,
                        proposal,
                        human,
                        &authority,
                        now_ms,
                        true,
                    )
                    .map_err(|e| e.to_string())?
                    {
                        athanor::workshop_destination::GraphPreparation::Existing(receipt) => {
                            (None, receipt, None)
                        },
                        athanor::workshop_destination::GraphPreparation::New { deltas, receipt } => {
                            (Some(deltas), receipt, None)
                        },
                    }
                };
                let (deltas, receipt, epoch) = prepared;
                let should_persist = deltas.is_some();
                if let Some(deltas) = deltas {
                    for delta in deltas {
                        if let mere::kernel::graph::apply::GraphDeltaResult::Refused(error) =
                            mere::kernel::graph::apply::apply_graph_delta(&mut candidate, delta)
                        {
                            return Err(error.to_string());
                        }
                    }
                }
                if should_persist && private_destination {
                    athanor::workshop_sealed_destination::verify_graph_effect(
                        &candidate,
                        &owner,
                        &dest.graph,
                        &acceptance,
                        proposal,
                        protection.as_ref(),
                    )
                    .map_err(|e| e.to_string())?;
                } else if should_persist {
                    athanor::workshop_destination::verify_graph_effect(
                        &candidate,
                        &owner,
                        &dest.graph,
                        &acceptance,
                        proposal,
                    )
                    .map_err(|e| e.to_string())?;
                }
                if epoch.is_some_and(|epoch| protection.epoch() != Some(epoch)) {
                    return Err("Profile custody changed while preparing the retained brief.".into());
                }
                Ok::<_, String>((receipt, should_persist, epoch))
            })
        };
        let (receipt, should_persist, prepared_epoch) = match result {
            Ok(result) => result,
            Err(error) => {
                window.status("Destination write not acknowledged.", Some(error));
                return;
            },
        };
        let Some(runtime) = self.app.graph_runtimes.get(graph_id) else {
            window.status(
                "Destination owner disappeared.",
                Some("The current graph runtime is unavailable.".into()),
            );
            return;
        };
        if should_persist {
            if prepared_epoch.is_some_and(|epoch| protection.epoch() != Some(epoch)) {
                window.status(
                    "Profile custody changed before the graph save.",
                    Some("The prepared private brief was not saved.".into()),
                );
                return;
            }
            if let Err(error) = athanor::workshop_destination::preserve_graph_operations(
                &authoritative_before,
                &candidate,
            ) {
                window.status(
                    "The retained operation ledger could not be preserved.",
                    Some(error.to_string()),
                );
                return;
            }
            let path = directory.join(pandect::session_graph_store::GRAPH_FILE);
            let saved = runtime.persistence.save_graph_with(
                &directory,
                &candidate,
                || pandect::save_node_facets(&directory, candidate.facets()),
                |placement| crate::session_graph_file::save_profiled(&path, &candidate, placement),
            );
            if let Err(error) = saved {
                if let Some(driver) = window.driver.as_mut() {
                    let _ = pollster::block_on(driver.workshop.uncertain(run));
                }
                window.status(
                    "Destination save outcome is uncertain; reconcile before retrying.",
                    Some(error.to_string()),
                );
                let refreshed = window
                    .driver
                    .as_mut()
                    .and_then(|driver| pollster::block_on(driver.refresh()).ok());
                if let Some(view) = refreshed {
                    window.update(|s| s.view = Some(view));
                }
                return;
            }
        }
        self.app
            .graph_runtimes
            .get_mut(graph_id)
            .unwrap()
            .canvas
            .set_graph(candidate);
        if let Err(error) = self.app.bind_behavior_journal() {
            self.app.refuse_behavior(error);
        }
        if should_persist
            && prepared_epoch.is_some_and(|epoch| protection.epoch() != Some(epoch))
        {
            window.status(
                "The graph was saved, but custody changed before acknowledgement.",
                Some("Reconcile the durable operation after unlocking the same profile.".into()),
            );
            return;
        }
        let acknowledged = match window.driver.as_mut() {
            Some(driver) => pollster::block_on(driver.workshop.acknowledge(run, receipt)),
            None => {
                window.status(
                    "The graph was saved; the receipt must be recovered.",
                    Some("The workshop driver is unavailable.".into()),
                );
                return;
            },
        };
        match acknowledged {
            Ok(()) => match window
                .driver
                .as_mut()
                .map(|driver| pollster::block_on(driver.refresh()))
            {
                Some(Ok(view)) => window.update(|s| {
                    s.view = Some(view);
                    s.status = "The graph snapshot and destination receipt were saved.".into();
                    s.error = None;
                }),
                Some(Err(error)) => window.status(
                    "The graph was saved; reload the workshop receipt.",
                    Some(error.to_string()),
                ),
                None => window.status(
                    "The graph was saved; reload the workshop receipt.",
                    Some("The workshop driver is unavailable.".into()),
                ),
            },
            Err(error) => window.status(
                "The graph was saved; the receipt must be recovered.",
                Some(error.to_string()),
            ),
        }
        self.request_redraw();
    }

    pub(super) fn alembic_workshop_event(&mut self, id: WindowId, event: &WindowEvent) -> bool {
        let Some(window) = self
            .alembic_workshop
            .as_mut()
            .filter(|w| w.host.native_window().is_some_and(|n| n.id() == id))
        else {
            return false;
        };
        window.host.handle_window_event(event.clone());
        self.request_redraw();
        true
    }

    /// Let the workshop finish its pause-and-close path before quitting the app.
    pub(super) fn request_close_with_alembic_workshop(&mut self) -> bool {
        let Some(window) = self.alembic_workshop.as_mut() else {
            return false;
        };
        self.close_after_alembic_workshop = true;
        window.host.request_close(CloseRequest::Native);
        if let Some(native) = window.host.native_window() {
            native.focus_window();
            native.request_redraw();
        }
        true
    }
}
