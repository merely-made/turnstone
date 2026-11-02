// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn handle_alembic_action(&mut self, action: SurfaceAction) {
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
                        let protection = self
                            .custody_binding
                            .as_ref()
                            .map(|binding| binding.protection.clone());
                        match pollster::block_on(driver.refresh()) {
                            Ok(view) => crate::alembic_workshop::proposal_reading(
                                &owner,
                                &view,
                                proposal,
                                &source,
                                protection.as_deref(),
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
                    Some(Err(error)) => {
                        self.status_alembic("Could not open the retained result.", Some(error))
                    },
                    None => self.status_alembic(
                        "No retained evidence brief was selected.",
                        Some("The workshop did not provide a result resource.".into()),
                    ),
                }
            },
        }
    }

    pub(super) fn status_alembic(&mut self, status: impl Into<String>, error: Option<String>) {
        if let Some(window) = self.alembic_workshop.as_mut() {
            window.status(status, error);
        }
    }

    pub(super) fn load_alembic_examples(&mut self) {
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
        let owner = self
            .alembic_workshop
            .as_ref()
            .map(|window| window.owner.clone());
        let choices = owner
            .as_ref()
            .map(|owner| self.alembic_workshop_choices(owner))
            .unwrap_or_default();
        self.status_alembic(
            format!("Public examples retained in session {}.", session.as_uuid()),
            None,
        );
        if let Some(window) = self.alembic_workshop.as_mut() {
            window.update(|state| {
                state.choices = choices;
                state.busy = false;
            });
        }
        self.request_redraw();
    }

    pub(super) fn run_alembic(&mut self) {
        let Some(window) = self.alembic_workshop.as_mut() else {
            return;
        };
        let Some(selected) = window.state().map(|state| state.selected()) else {
            return;
        };
        let owner = window.owner.clone();
        let installation = window.installation;
        let human = window.human;
        if owner.session != self.app.session_id.as_uuid().to_string()
            || owner.workspace != self.app.graph_runtimes.active_graph().as_uuid().to_string()
        {
            window.status(
                "The current session or graph changed.",
                Some("Reopen the workshop for the current owner.".into()),
            );
            return;
        }
        let entries = self.app.saved_sources.entries.clone();
        let current_admissions = self.app.saved_source_admissions();
        let protection = self
            .custody_binding
            .as_ref()
            .map(|binding| binding.protection.clone());
        let Some(protection) = protection else {
            window.status("Profile custody is unavailable.", None);
            return;
        };
        let Some(graph) = self
            .app
            .graph_runtimes
            .get(self.app.graph_runtimes.active_graph())
            .map(|r| r.canvas.graph())
        else {
            return;
        };
        let passages = match crate::alembic_workshop::capture_selected_sources(
            graph,
            &owner,
            &selected,
            &entries,
            &current_admissions,
            protection.as_ref(),
        ) {
            Ok(passages) => passages,
            Err(error) => {
                window.status(
                    "The selected retained sources could not be admitted.",
                    Some(error),
                );
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
        let app = &self.app;
        let source_gate = |passage: &alembic::workshop::Passage| {
            workshop_passage_current(app, &owner, Some(protection.as_ref()), passage)
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
                state.status = "Generating from the selected retained passages.".into();
                state.error = None;
            }),
            Err(error) => window.status(
                "The selected sources were refused or could not start.",
                Some(error.to_string()),
            ),
        }
    }

    pub(super) fn control_alembic(&mut self, control: Control) {
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
                    workshop_passage_current(
                        &self.app,
                        &owner,
                        self.custody_binding
                            .as_ref()
                            .map(|binding| binding.protection.as_ref()),
                        p,
                    )
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
}
