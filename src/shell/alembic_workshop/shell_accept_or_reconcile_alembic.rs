// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn accept_or_reconcile_alembic(&mut self, reconcile: bool) {
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
            let disk_revision =
                match athanor::workshop_destination::graph_revision(&authoritative_before) {
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
                        !workshop_passage_current(
                            &self.app,
                            &owner,
                            Some(protection.as_ref()),
                            passage,
                        )
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
                        let epoch = protection
                            .epoch()
                            .ok_or_else(|| "Profile custody is unavailable.".to_string())?;
                        return Ok((receipt, false, Some(epoch)));
                    }
                }
                if proposal.sources.iter().any(|passage| {
                    !workshop_passage_current(&self.app, &owner, Some(protection.as_ref()), passage)
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
                        athanor::workshop_destination::GraphPreparation::New {
                            deltas,
                            receipt,
                        } => (Some(deltas), receipt, None),
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
                    return Err(
                        "Profile custody changed while preparing the retained brief.".into(),
                    );
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
        let acknowledged = install_destination_then_acknowledge(
            || {
                self.app
                    .graph_runtimes
                    .get_mut(graph_id)
                    .unwrap()
                    .canvas
                    .set_graph(candidate);
                if let Err(error) = self.app.bind_behavior_journal() {
                    self.app.refuse_behavior(error);
                }
            },
            || {
                if should_persist
                    && prepared_epoch.is_some_and(|epoch| protection.epoch() != Some(epoch))
                {
                    return Err("The graph was saved, but custody changed before acknowledgement. Reconcile the durable operation after unlocking the same profile.".to_string());
                }
                let driver = window
                    .driver
                    .as_mut()
                    .ok_or_else(|| "The workshop driver is unavailable.".to_string())?;
                pollster::block_on(driver.workshop.acknowledge(run, receipt.clone()))
                    .map_err(|error| error.to_string())
            },
        );
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

    pub(in super::super) fn alembic_workshop_event(
        &mut self,
        id: WindowId,
        event: &WindowEvent,
    ) -> bool {
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
    pub(in super::super) fn request_close_with_alembic_workshop(&mut self) -> bool {
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
