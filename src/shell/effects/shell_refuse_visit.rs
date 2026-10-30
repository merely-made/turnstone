// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Record a refusal and tell the app, without disturbing the node.
    pub(super) fn refuse_visit(&mut self, node: uuid::Uuid, url: &str, reason: &str) {
        self.app.note_place_refusal(format!("{url}: {reason}"));
        let effects = self.app.apply_update(Update::ContentFailed {
            node,
            error: format!("this mere cannot reach the document at {url}: {reason}"),
        });
        self.run_effects(effects);
        self.knot_visit_refusals
            .retain(|facts| facts.address != url);
        self.knot_visit_refusals
            .push(crate::observe::KnotDocumentFacts {
                address: url.to_string(),
                holder: crate::knot_authoring::parse_place_held_address(url)
                    .map(|(root, _)| crate::place::hex32(&root)),
                status: "refused".into(),
                derived_digest: None,
                retained_selection: None,
            });
    }

    /// Fold one prepared dial: open the visiting hub, then replay every spawn
    /// that was waiting on this holder.
    pub(in super::super) fn apply_document_visit(
        &mut self,
        holder_root: [u8; 32],
        result: Result<Box<crate::place::lanes::HolderDial>, String>,
    ) {
        let waiting: Vec<(uuid::Uuid, String)> = {
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_visit_spawns)
                .into_iter()
                .partition(|(root, _, _)| *root == holder_root);
            self.pending_visit_spawns = rest;
            mine.into_iter().map(|(_, node, url)| (node, url)).collect()
        };
        let proxy = self.proxy.clone();
        let wake: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
            let _ = proxy.send_event(());
        });
        let opened = result.and_then(|dial| crate::knot_authoring::visit_holder(*dial, wake));
        match opened {
            Ok(hub) => {
                if let Ok(mut visits) = self.knot_visits.lock() {
                    visits.insert_holder(holder_root, hub);
                }
                for (node, url) in waiting {
                    self.run_effects(vec![crate::action::Effect::SpawnContent { node, url }]);
                }
            },
            Err(error) => {
                for (node, url) in waiting {
                    self.refuse_visit(node, &url, &error);
                }
            },
        }
    }

    /// Re-mirror what the live Knot surfaces hold, plus what was refused.
    ///
    /// Also carries this profile's own root into the visit registry, which is
    /// what makes a place-held address naming this mere resolve locally.
    pub(in super::super) fn refresh_knot_documents(&mut self) {
        let local_root = match &self.app.place {
            crate::place::PlaceState::Offline { snapshot, .. } => Some(snapshot.personae_root),
            _ => None,
        };
        if let Ok(mut visits) = self.knot_visits.lock() {
            visits.set_local_root(local_root);
        }
        let mut facts: Vec<crate::observe::KnotDocumentFacts> = self
            .content_sessions
            .sessions()
            .filter_map(|session| {
                let knot = session
                    .as_any_ref()
                    .downcast_ref::<crate::knot_authoring::KnotDocumentSession>()?;
                Some(crate::observe::KnotDocumentFacts {
                    address: knot.address().to_string(),
                    holder: knot.holder(),
                    status: knot.hold_status().to_string(),
                    derived_digest: Some(knot.derived_digest()),
                    retained_selection: knot.retained_selection(),
                })
            })
            .collect();
        let refused: Vec<crate::observe::KnotDocumentFacts> = self
            .knot_visit_refusals
            .iter()
            .filter(|refusal| !facts.iter().any(|open| open.address == refusal.address))
            .cloned()
            .collect();
        facts.extend(refused);
        facts.sort_by(|left, right| left.address.cmp(&right.address));
        self.app.set_knot_documents(facts);
    }

    pub(in super::super) fn release_place_worker(&mut self) {
        if let Err(error) = self.release_place_worker_result() {
            tracing::warn!(%error, "place worker release ack timed out");
        }
    }

    pub(super) fn release_place_worker_result(&mut self) -> Result<(), String> {
        // Every visiting hub drives its carrier on the lane runtime the
        // worker is about to drop. Letting go first is what keeps a visit
        // from reading a runtime that is shutting down under it.
        if let Ok(mut visits) = self.knot_visits.lock() {
            visits.clear();
        }
        self.pending_visit_spawns.clear();
        self.knot_visit_refusals.clear();
        let (ack_tx, ack_rx) = std::sync::mpsc::sync_channel(1);
        self.place_handle
            .command(crate::place::worker::PlaceWorkerCommand::Release(ack_tx));
        if ack_rx
            .recv_timeout(std::time::Duration::from_millis(1500))
            .is_err()
        {
            return Err("place worker release acknowledgement timed out".into());
        }
        Ok(())
    }

    /// Scene edits settle in a rendered owner pane before their geometry is
    /// saved. Both native windows call this after that pane's frame.
    pub(in super::super) fn save_ready_graph_scene(&mut self, pane: crate::panes::PaneId) {
        let Some(graph) = self.app.graph_for_pane(pane) else {
            return;
        };
        let ready = self.app.graph_runtimes.get(graph).is_some_and(|r| {
            r.scene_save_pending && r.scene_transition.is_none() && !r.canvas.has_stop_return()
        });
        if !ready {
            return;
        }
        // A refusal is reported once; another explicit edit can retry.
        self.app
            .graph_runtimes
            .get_mut(graph)
            .unwrap()
            .scene_save_pending = false;
        if let Err(error) = self.app.persist_graph_scene(graph) {
            tracing::warn!(%error, "graph scene persistence refused or failed");
            self.app.note(crate::observe::AppEvent::InteractionMissed {
                what: "graph-scene-save",
                target: error.to_string(),
            });
        }
    }

    /// Save the resolved graph runtime's curation beside its own session.
    pub(in super::super) fn save_graph_folds(&mut self, graph: crate::panes::GraphId) {
        let Some(runtime) = self.app.graph_runtimes.get(graph) else {
            return;
        };
        let Some(session) = runtime.session else {
            return;
        };
        let dir = session::session_dir(&self.app.data_root, session);
        if let Err(error) = runtime.persistence.placement_for(&dir) {
            tracing::warn!(%error, "fold view persistence has no session authority");
            return;
        }
        if let Err(error) = crate::scene_state::save_view(
            &dir,
            &runtime.canvas,
            &self.app.command_choices,
            &runtime.view,
        ) {
            tracing::warn!(%error, "fold view persistence refused or failed");
        }
    }

    /// Persist the live session's whole sidecar set under ITS directory
    /// (`sessions/<id>/`) — the SaveSession effect's body, shared by the
    /// session switch (which must save the DEPARTING session first).
    pub(in super::super) fn save_session(&mut self) {
        // Explicit save/close also captures views whose panes have just closed.
        // Each write rechecks that runtime's own session authority.
        for graph in self.app.graph_runtimes.pending_scene_graphs() {
            self.app
                .graph_runtimes
                .get_mut(graph)
                .unwrap()
                .scene_save_pending = false;
            if let Err(error) = self.app.persist_graph_scene(graph) {
                tracing::warn!(%error, "graph scene save refused or failed");
            }
        }
        if let Err(reason) = self.app.prepare_shared_stations_for_save() {
            tracing::warn!(%reason,"pane station persistence refused; session save skipped");
            self.app.note(crate::observe::AppEvent::InteractionMissed {
                what: "save-pane-stations",
                target: reason,
            });
            return;
        }
        let sdir = self.app.session_dir();
        if let Err(error) = self.app.persist_session_graph() {
            tracing::warn!(%error, "session persistence refused or failed; sidecars and image collection skipped");
            return;
        }
        if let Some(binding) = self.app.place.binding() {
            match session::update_place_binding(&sdir, binding) {
                Ok(true) => {},
                // Loud rather than silent: app state holding a binding with no
                // admitted sidecar means something bound a place without going
                // through admission, which is the exact ordering T3a inverts.
                Ok(false) => tracing::warn!(
                    "place state carries a binding with no admitted place.json; \
                     refusing to create one outside admission"
                ),
                Err(error) => tracing::warn!(%error, "failed to persist place binding"),
            }
        }
        let swept = session::gc_orphan_image_blobs(&sdir, self.app.graph_runtimes.graph());
        if swept > 0 {
            tracing::info!(swept, "reclaimed orphaned session image blobs");
        }
        // The shared pane stations persist to workspace.json alongside the graph
        // (rung 5 slice C), so summon/close/divider survive a restart.
        session::save_frisket_layout(&sdir, &self.app.frisket);
        // The workbench tiling persists as platen's canonical pair
        // (rung 5 slice E), so tiles/stacks/fractions survive too.
        if let Some(workbench) = self.app.active_workbench() {
            session::save_workbench(&sdir, workbench);
        }
        // The lens-window spaces (rung 7 depth): torn-out panes
        // survive a restart as windows again.
        session::save_lens_spaces(&sdir, &self.app.lenses);
        // Browser, canonical facets and graph were persisted together above.
        if let Err(error) = self.app.gemini_identities.save(&self.app.data_root) {
            tracing::warn!(%error, "failed to persist Gemini identity bindings");
        }
        if let Some(score) = self.app.graph_runtimes.projection_score() {
            session::save_projection_score(&sdir, score);
        }
        self.save_graph_folds(self.app.graph_runtimes.active_graph());
        // Stamp a derived display name the first time the session has content
        // to name it after (unset -> "Example Domain"), then bump recency so
        // the switcher orders by last-used. Derive before the mutable borrow.
        let id = self.app.session_id;
        let derived = self
            .app
            .sessions
            .get(id)
            .is_some_and(|m| m.display_name.is_none())
            .then(|| self.app.derive_session_name())
            .flatten();
        if self.app.sessions.update(id, |m| {
            if m.display_name.is_none()
                && let Some(name) = derived.clone()
            {
                m.display_name = Some(name);
            }
            m.touch();
        }) {
            let _ = self.app.sessions.flush_dirty();
        }
    }
}
