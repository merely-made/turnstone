// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn refuse_session_adoption(
        &mut self,
        graph: GraphId,
        directory: PathBuf,
        error: String,
    ) {
        self.graph_runtimes.adopt_graph(
            mere::kernel::graph::Graph::new(),
            crate::session_persistence::SessionPersistence::refused(directory, error.clone()),
        );
        self.next_place_generation = self.next_place_generation.wrapping_add(1);
        self.place = crate::place::PlaceState::Failed {
            error: error.clone(),
        };
        self.feeds = Default::default();
        self.redshank = self.redshank.suspended();
        self.redshank_members.clear();
        self.browser = Default::default();
        self.content = ContentStates::default();
        self.removed.clear();
        self.resident_runs = Default::default();
        self.resident_run_error = Some(error.clone());
        self.events.push(AppEvent::SessionLoadRefused { error });
        self.graph_runtimes.suspend_recording(graph);
    }

    pub(crate) fn persist_session_graph(&mut self) -> std::io::Result<()> {
        let directory = self.session_dir();
        self.graph_runtimes
            .active_persistence()
            .placement_for(&directory)?;
        self.refresh_browser_states();
        self.refresh_facets();
        crate::content_classes::reconcile(&mut self.graph_runtimes)
            .map_err(std::io::Error::other)?;
        // Persist the sidecar's read status before its Surface projection. An
        // error is observable and stops this save rather than reporting success.
        self.feeds.save(&directory)?;
        self.persist_owned_graph()
    }

    pub(crate) fn persist_owned_graph(&self) -> std::io::Result<()> {
        let directory = self.session_dir();
        let graph = self.graph_runtimes.graph();
        self.graph_runtimes.active_persistence().save_graph_with(
            &directory,
            graph,
            || pandect::save_node_facets(&directory, graph.facets()),
            |placement| {
                crate::session_graph_file::save_profiled(
                    &directory.join(pandect::session_graph_store::GRAPH_FILE),
                    graph,
                    placement,
                )
            },
        )
    }

    /// Refresh the browser-state sidecar from live truth before a save
    /// (rung 6): each graph node's `content_on` mirrors its content
    /// lifecycle (live or in flight), and entries for vanished nodes drop.
    pub fn refresh_browser_states(&mut self) {
        use crate::content::NodeContent;
        let present: std::collections::HashSet<uuid::Uuid> = self
            .graph_runtimes
            .graph()
            .nodes()
            .map(|(_, n)| n.id)
            .collect();
        let stale: Vec<uuid::Uuid> = self
            .browser
            .nodes
            .keys()
            .copied()
            .filter(|id| !present.contains(id))
            .collect();
        for id in stale {
            self.browser.remove(id);
        }
        for id in present {
            let on = matches!(
                self.content.get(id),
                Some(
                    NodeContent::Live
                        | NodeContent::Requested
                        | NodeContent::AwaitingInput
                        | NodeContent::AwaitingIdentity
                        | NodeContent::AwaitingTrust
                )
            );
            if on || self.browser.get(id).is_some() {
                self.browser.entry(id).content_on = on;
            }
        }
    }

    pub(in super::super) fn recover_session(&mut self, id: SessionId) -> Vec<Effect> {
        // Overmap O3 recovery: the trashed directory moves back whole
        // (graph + facets + bin), the manifest re-lists, and the
        // ordinary switch adopts it — same identity by construction.
        match self.sessions.restore_from_trash(id) {
            Ok(true) => {
                self.trash = self.sessions.list_trash();
                self.events
                    .push(AppEvent::SessionRecovered(self.session_label(id)));
                vec![Effect::SwitchSession { id }]
            },
            Ok(false) => {
                tracing::warn!(session = %id.as_uuid(), "no trash entry to recover");
                vec![Effect::Redraw]
            },
            Err(err) => {
                tracing::warn!(%err, "failed to recover the trashed session");
                vec![Effect::Redraw]
            },
        }
    }

    pub(in super::super) fn close_session(&mut self) -> Vec<Effect> {
        // Trash the current session, then land on the newest remaining
        // one; if it was the last, mint a fresh empty session. Either
        // way the switch effect saves nothing for the trashed session
        // (it is already gone) and adopts the target.
        let closing = self.session_id;
        let next = self
            .sessions
            .iter()
            .filter(|(id, _)| *id != closing)
            .max_by_key(|(_, m)| m.updated_at)
            .map(|(id, _)| id)
            .unwrap_or_else(|| Self::mint_session(&self.data_root, &mut self.sessions));
        // The disk half (bin release + trash move + adopt-without-save)
        // is ordering the SHELL owns — see Effect::TrashSession.
        vec![Effect::TrashSession { closing, next }]
    }

    pub(in super::super) fn begin_rename_session(&mut self) -> Vec<Effect> {
        // Seed empty (the omnibar has no selection, so a seeded label
        // could not be replaced by typing); the current label shows in
        // the switcher, and an empty commit clears back to it.
        self.omnibar = OmnibarState {
            open: true,
            mode: crate::ui::OmnibarMode::RenameSession(self.session_id),
            ..OmnibarState::default()
        };
        let target = self.fallback_shell_context();
        self.shell.begin_omnibar(target);
        self.focus = FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        self.events.push(AppEvent::OmnibarOpened);
        vec![Effect::Redraw]
    }

    pub(in super::super) fn rename_session(&mut self, id: SessionId, name: String) -> Vec<Effect> {
        let name = name.trim().to_string();
        let applied = self.sessions.update(id, |m| {
            m.display_name = (!name.is_empty()).then(|| name.clone());
        });
        if applied {
            let _ = self.sessions.flush_dirty();
            self.events
                .push(AppEvent::SessionRenamed(self.session_label(id)));
        }
        vec![Effect::Redraw]
    }

    pub(in super::super) fn empty_recycle_bin(&mut self) -> Vec<Effect> {
        // Athanor's oven, on command: the bin actor clears its store
        // and answers with the empty list (which refreshes the mirror).
        // A no-op when the bin is already empty (honest — no event).
        if self.removed.is_empty() {
            return vec![Effect::Redraw];
        }
        self.events
            .push(AppEvent::RecycleBinEmptied(self.removed.len()));
        vec![Effect::EmptyRecycleBin, Effect::Redraw]
    }

    /// Resolve the visible image misses the canvas observed in its last frame.
    /// Content-addressing makes duplicate references one request, and the
    /// canvas's byte-bounded LRU will ask again if an evicted digest later
    /// returns to view.
    pub(crate) fn resolve_pending_images(&mut self) -> usize {
        let refs = self.graph_runtimes.take_image_requests();
        if refs.is_empty() {
            return 0;
        }
        let sdir = self.session_dir();
        let mut loaded = 0;
        for image in refs {
            let Some(bytes) = session::load_image_blob(&sdir, &image.hex()) else {
                continue;
            };
            let Some(decoded) = crate::browse::decode_image_bytes(&bytes) else {
                continue;
            };
            self.graph_runtimes.register_resolved_image(
                image.digest,
                decoded.rgba,
                decoded.width,
                decoded.height,
            );
            loaded += 1;
        }
        loaded
    }
}
