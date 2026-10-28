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
                        driver: Some(Driver::new(backend, owner.clone(), install, wake)),
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
            window
                .host
                .s
                .runner
                .as_mut()
                .map(|runner| runner.update(|s| s.take_actions()))
                .unwrap_or_default()
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
            SurfaceAction::OpenSource { source, .. } => {
                let graph = self.app.graph_runtimes.graph();
                match crate::alembic_workshop::read_public_fixture(graph, &source) {
                    Ok(reading) => {
                        if let Some(window) = self.alembic_workshop.as_mut() {
                            window.update(|state| state.reading = Some(reading));
                        }
                    },
                    Err(error) => {
                        self.status_alembic("The cited source could not be reopened.", Some(error))
                    },
                }
            },
            SurfaceAction::OpenResult(resource) => {
                let result = self.alembic_workshop.as_ref().and_then(|window| {
                    let view = window.state()?.view.as_ref()?;
                    let Some(Review::Added { acceptance, receipt }) = view.review.as_ref() else {
                        return None;
                    };
                    if receipt.resource != resource
                        || receipt.operation != acceptance.operation
                        || receipt.proposal != acceptance.proposal
                        || receipt.destination != acceptance.destination
                        || acceptance.destination.owner != window.owner
                        || acceptance.destination.graph != window.owner.workspace
                        || view.installation != acceptance.installation
                        || view.run != Some(acceptance.run)
                    {
                        return Some(Err("The current owner receipt does not match this result."));
                    }
                    let original = athanor::workshop_destination::result_surface(acceptance);
                    let Some(runtime) = self
                        .app
                        .graph_runtimes
                        .iter()
                        .find(|runtime| runtime.session == Some(self.app.session_id))
                    else {
                        return Some(Err("The original evidence brief surface was removed; another surface cannot substitute."));
                    };
                    if runtime.graph.as_uuid().to_string() != acceptance.destination.graph {
                        return Some(Err("The original evidence brief surface was removed; another surface cannot substitute."));
                    }
                    let Some((key, node)) = runtime.canvas.graph().get_node_by_id(original) else {
                        return Some(Err("The original evidence brief surface was removed; another surface cannot substitute."));
                    };
                    let expected_resource = uuid::Uuid::parse_str(&resource).ok();
                    if expected_resource.is_none()
                        || runtime.canvas.graph().shown_resource_id(key) != expected_resource
                    {
                        return Some(Err("The original evidence brief surface was removed; another surface cannot substitute."));
                    }
                    Some(Ok(node.url.clone()))
                });
                match result {
                    Some(Ok(address)) => {
                        self.act(crate::action::Action::OpenAddress(address));
                        self.status_alembic(
                            "Opened the retained evidence brief in the owner graph.",
                            None,
                        );
                    },
                    Some(Err(error)) => self
                        .status_alembic("Could not open the retained result.", Some(error.into())),
                    None => self.status_alembic(
                        "No current Added receipt authorizes this result.",
                        Some(
                            "Reopen the workshop's current owner review before opening it.".into(),
                        ),
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
            |placement| pandect::session_graph_store::save_profiled(&path, &candidate, placement),
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
            format!("Public examples retained in session {}.", session),
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
        let disk_graph = disk_graph.unwrap_or_else(|| live_graph.clone());
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
            let disk_revision = match athanor::workshop_destination::graph_revision(&disk_graph) {
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
        let mut candidate = disk_graph;
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
                if reconcile {
                    if let Some(receipt) = athanor::workshop_destination::query_graph(
                        &candidate,
                        &owner,
                        &dest.graph,
                        &acceptance,
                        proposal,
                    )
                    .map_err(|e| e.to_string())?
                    {
                        driver
                            .workshop
                            .acknowledge(run, receipt)
                            .await
                            .map_err(|e| e.to_string())?;
                        return Ok((receipt, false));
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
                let revision = athanor::workshop_destination::graph_revision(&candidate)
                    .map_err(|e| e.to_string())?;
                let prepared = athanor::workshop_destination::prepare_graph(
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
                .map_err(|e| e.to_string())?;
                let athanor::workshop_destination::GraphPreparation::New { deltas, receipt } =
                    prepared
                else {
                    return Err(
                        "The graph records the operation but the run receipt needs reconciliation."
                            .into(),
                    );
                };
                for delta in deltas {
                    mere::kernel::graph::apply::apply_graph_delta(&mut candidate, delta);
                }
                athanor::workshop_destination::verify_graph_effect(
                    &candidate,
                    &owner,
                    &dest.graph,
                    &acceptance,
                    proposal,
                )
                .map_err(|e| e.to_string())?;
                Ok::<_, String>((receipt, true))
            })
        };
        let (receipt, should_persist) = match result {
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
            let path = directory.join(pandect::session_graph_store::GRAPH_FILE);
            let saved = runtime.persistence.save_graph_with(
                &directory,
                &candidate,
                || pandect::save_node_facets(&directory, candidate.facets()),
                |placement| {
                    pandect::session_graph_store::save_profiled(&path, &candidate, placement)
                },
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
        match pollster::block_on(driver.workshop.acknowledge(run, receipt)) {
            Ok(()) => match pollster::block_on(driver.refresh()) {
                Ok(view) => window.update(|s| {
                    s.view = Some(view);
                    s.status = "The graph snapshot and destination receipt were saved.".into();
                    s.error = None;
                }),
                Err(error) => window.status(
                    "The graph was saved; reload the workshop receipt.",
                    Some(error.to_string()),
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
