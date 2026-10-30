// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    pub(super) fn start_private_connect(&mut self) {
        if let Some((setup, _)) = self.custody_setup.take() {
            setup.request_stop();
            self.retired_setups.push(setup);
        }
        match crate::custody_binding::Setup::connect(
            self.custody_generation.clone(),
            self.custody_wake.clone(),
        ) {
            Ok((setup, events)) => self.custody_setup = Some((setup, events)),
            Err(error) => {
                tracing::warn!(%error, "profile custody connection remains pending");
            },
        }
    }

    /// Service bounded connection and loss notifications before accepting any
    /// private actor reply. The epoch is local and cheap to inspect here.
    pub(super) fn service_private_custody(&mut self) {
        self.retired_setups.retain(|setup| !setup.is_finished());
        self.retired_watchers
            .retain(|watcher| !watcher.is_finished());

        let lost = self.custody_binding.as_ref().is_some_and(|binding| {
            binding.bound.epoch().is_none()
                || !matches!(
                    binding.events.try_recv(),
                    Err(std::sync::mpsc::TryRecvError::Empty)
                )
        });
        if lost {
            self.lose_private_custody();
        }

        let result = self
            .custody_setup
            .as_ref()
            .and_then(|(_, rx)| rx.try_recv().ok());
        if let Some(result) = result {
            if let Some((setup, _)) = self.custody_setup.take() {
                self.retired_setups.push(setup);
            }
            match result {
                crate::custody_binding::SetupEvent::Ready {
                    root,
                    bound,
                    watcher,
                    events,
                } => {
                    let principal = root.custody_principal();
                    let expected_matches = match &self.custody_expected_principal {
                        Some(expected) => principal.as_ref() == Some(expected),
                        None => self.app.identity.is_none(),
                    };
                    if !expected_matches || principal.is_none() || bound.epoch().is_none() {
                        bound.retire();
                        watcher.request_stop();
                        self.retired_watchers.push(watcher);
                        tracing::warn!(
                            "fresh custody connection did not match the expected profile; private access remains pending"
                        );
                    } else {
                        self.adopt_private_binding(
                            root,
                            bound,
                            watcher,
                            events,
                            principal.unwrap(),
                        );
                    }
                },
                crate::custody_binding::SetupEvent::Refused => {
                    tracing::warn!(
                        "fresh profile custody connection was refused; private access remains pending"
                    );
                },
            }
        } else if self
            .custody_setup
            .as_ref()
            .is_some_and(|(setup, _)| setup.is_finished())
        {
            self.custody_setup = None;
            tracing::warn!(
                "profile custody connection ended without a result; private access remains pending"
            );
        }
        self.service_place_after_custody();
    }

    pub(super) fn service_place_after_custody(&mut self) {
        let Some(result) = self
            .custody_trail_ready
            .as_ref()
            .map(std::sync::mpsc::Receiver::try_recv)
        else {
            return;
        };
        match result {
            Ok(()) => {},
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.custody_trail_ready = None;
                self.pending_place_reopen = None;
                self.app.place = crate::place::PlaceState::Failed {
                    error: "the source actor stopped before captured-source custody was ready"
                        .into(),
                };
                tracing::warn!(
                    "place reopen refused because the source actor stopped before custody readiness"
                );
                return;
            },
        }
        self.custody_trail_ready = None;
        let Some((session, generation, binding)) = self.pending_place_reopen.take() else {
            return;
        };
        let directory = crate::session::session_dir(&self.app.data_root, session);
        if !self
            .place_handle
            .command(crate::place::worker::PlaceWorkerCommand::Open {
                session,
                generation,
                directory,
                binding: binding.clone(),
            })
        {
            self.app.place = crate::place::PlaceState::Degraded {
                binding,
                generation,
                error: "the place worker refused the fresh custody open".into(),
            };
        }
    }

    pub(super) fn retire_place_for_custody(&mut self) {
        self.capture_library.retire_custody();
        self.custody_trail_ready = None;
        self.pending_place_reopen = None;
        self.app.next_place_generation = self.app.next_place_generation.wrapping_add(1);
        let generation = self.app.next_place_generation;
        if let Some(binding) = self.app.place.binding().cloned() {
            self.app.place = crate::place::PlaceState::Opening {
                binding: binding.clone(),
                generation,
            };
            self.pending_place_reopen = Some((self.app.session_id, generation, binding));
        } else if matches!(&self.app.place, crate::place::PlaceState::Joining { .. }) {
            // A join can still be in flight in another owner when custody is
            // lost. Rotate its generation and close the visible admission so
            // stale replies cannot publish under the departed identity.
            self.app.place = crate::place::PlaceState::Failed {
                error: "profile custody was lost during place admission".into(),
            };
        }
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        if !self
            .place_handle
            .command(crate::place::worker::PlaceWorkerCommand::RetireIdentity(
                ack_tx,
            ))
            || ack_rx
                .recv_timeout(std::time::Duration::from_millis(1500))
                .is_err()
        {
            self.pending_place_reopen = None;
            self.app.place = crate::place::PlaceState::Failed {
                error: "the previous place identity could not be retired".into(),
            };
            tracing::warn!("place identity retirement acknowledgement timed out");
        }
    }

    pub(super) fn adopt_private_binding(
        &mut self,
        root: Arc<crate::identity::RootIdentity>,
        bound: Arc<crate::custody_binding::BoundSealer>,
        watcher: crate::custody_binding::Watcher,
        events: Receiver<crate::custody_binding::Event>,
        principal: crate::identity::CustodyPrincipal,
    ) {
        let epoch_bound = bound.clone();
        let protection: Arc<dyn athanor::workshop_seal::Protection> = Arc::new(
            athanor::workshop_seal::EideticProtection::new(bound.clone(), move || {
                epoch_bound.epoch()
            }),
        );
        self.app
            .research
            .access(Some(crate::research::ClipAccess::from_bound(
                self.app.data_root.clone(),
                bound.clone(),
            )));
        let source_epoch = bound.clone();
        let source_custody = crate::trail_memory::SourceCustody {
            sealer: bound.clone(),
            epoch: Arc::new(move || source_epoch.epoch()),
        };
        if !self
            .trail_handle
            .command(crate::trail_memory::TrailCommand::SetCustody(Some(
                source_custody,
            )))
        {
            self.app.research.access(None);
            bound.retire();
            watcher.request_stop();
            self.retired_watchers.push(watcher);
            tracing::warn!(
                "trail actor refused the fresh custody binding; private access remains pending"
            );
            return;
        }
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        if !self
            .trail_handle
            .command(crate::trail_memory::TrailCommand::CustodyReady(ready_tx))
        {
            self.app.research.access(None);
            bound.retire();
            watcher.request_stop();
            self.retired_watchers.push(watcher);
            tracing::warn!(
                "trail actor refused the custody readiness barrier; private access remains pending"
            );
            return;
        }
        self.custody_trail_ready = Some(ready_rx);

        self.custody_expected_principal = Some(principal);
        let effects = self.app.adopt_profile_root(root.clone());
        let _ = self
            .place_handle
            .command(crate::place::worker::PlaceWorkerCommand::AdoptIdentity(
                root.clone(),
            ));
        if !self.network_offline {
            self.shared_knot_service = start_share_reader(root.clone());
        }
        if let Some(source) = self
            .pending_publish_source
            .take()
            .filter(|_| !self.network_offline)
        {
            self.publish_service = start_publishing(source, root.clone());
        }
        self.renderers.publish.clear();
        self.renderers.shared_knot.clear();
        self.custody_binding = Some(PrivateBinding {
            root,
            bound,
            protection,
            watcher,
            events,
        });
        self.app.research.set_available(true);
        self.app.sync_research_context();
        if let Some(owner) = self.app.personae_root() {
            let owner: String = owner.iter().map(|b| format!("{b:02x}")).collect();
            for (url, transition, at_ms) in std::mem::take(&mut self.pending_trail) {
                self.trail_handle
                    .command(crate::trail_memory::TrailCommand::Record {
                        owner: owner.clone(),
                        url,
                        transition,
                        at_ms,
                    });
            }
        }
        self.run_effects(effects);
        self.request_redraw();
    }

    pub(super) fn lose_private_custody(&mut self) {
        if let Some(principal) = self
            .custody_binding
            .as_ref()
            .and_then(|binding| binding.root.custody_principal())
        {
            self.custody_expected_principal = Some(principal);
        }
        crate::custody_binding::advance_generation(&self.custody_generation);
        if let Some(binding) = self.custody_binding.take() {
            binding.bound.retire();
            binding.watcher.request_stop();
            self.retired_watchers.push(binding.watcher);
        }
        if let Some((setup, _)) = self.custody_setup.take() {
            setup.request_stop();
            self.retired_setups.push(setup);
        }
        self.app.research.set_available(false);
        self.retire_place_for_custody();
        self.trail_handle
            .command(crate::trail_memory::TrailCommand::SetCustody(None));
        drop(self.alembic_workshop.take());
        let effects = self.app.retire_private_custody();
        let directory = crate::trail_memory::memory_dir(&self.app.session_dir());
        self.trail_handle
            .command(crate::trail_memory::TrailCommand::Reopen {
                directory,
                capture_generation: self.app.source_capture_generation,
            });
        let answers: Vec<_> = self
            .renderers
            .contributed
            .iter()
            .filter(|(_, pane)| {
                pane.descriptor().surface_id
                    == mere_surface_api::SurfaceId::from(crate::inference_surface::SOURCE_SCHEMA)
            })
            .map(|(id, _)| id)
            .collect();
        for pane in answers {
            self.renderers.contributed.remove(pane);
        }
        self.run_effects(effects);
        self.request_redraw();
        self.start_private_connect();
    }
}
