// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Browser commands follow the active pane's published member. The graph
    /// cursor remains the fallback when no pane supplies a member.
    pub(crate) fn browser_command_target(&self) -> Option<(Uuid, String)> {
        let content_member = match self.focus {
            FocusTarget::Content { node, .. } => Some(node),
            _ => None,
        };
        let member = content_member
            .or_else(|| {
                self.active_pane
                    .and_then(|pane| {
                        self.pane_context
                            .resolve_context(
                                pane,
                                &crate::panes::ContextBinding::Own,
                                crate::panes::SourceSelector::Graph,
                            )
                            .or_else(|| self.follower_context(pane))
                    })
                    .and_then(|context| context.member)
            })
            .or_else(|| self.graph_runtimes.focused_member())?;
        let canvas = self
            .graph_runtimes
            .graph_containing_member(member)
            .and_then(|graph| self.graph_runtimes.canvas(graph))?;
        let (_, node) = canvas.graph().get_node_by_id(member)?;
        Some((member, node.url().to_string()))
    }

    pub(crate) fn focused_can_back(&self) -> bool {
        self.browser_command_target()
            .is_some_and(|(node, _)| self.member_can_back(node))
    }

    pub(crate) fn member_can_back(&self, node: Uuid) -> bool {
        if self.content.surface_traversal_pending(node) {
            return false;
        }
        self.graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .is_some_and(|canvas| canvas.member_can_back(node))
    }

    pub(crate) fn focused_can_forward(&self) -> bool {
        self.browser_command_target()
            .is_some_and(|(node, _)| self.member_can_forward(node))
    }

    pub(crate) fn member_can_forward(&self, node: Uuid) -> bool {
        if self.content.surface_traversal_pending(node) {
            return false;
        }
        self.graph_runtimes
            .graph_containing_member(node)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .is_some_and(|canvas| canvas.member_can_forward(node))
    }

    /// Durable Keep belongs to one Surface. Showing the same Resource does
    /// not transfer another view's retention choice.
    pub(crate) fn node_is_kept(&self, member: Uuid) -> bool {
        self.graph_runtimes
            .graph_containing_member(member)
            .and_then(|graph| self.graph_runtimes.canvas(graph))
            .is_some_and(|canvas| {
                crate::surface_controls::read(
                    canvas.graph(),
                    member,
                    crate::surface_controls::Control::Keep,
                )
            })
    }

    /// Promote one captured member into durable kept state. Idempotence makes
    /// repeated UI or automation delivery harmless. Release belongs to the
    /// retention-policy lane rather than being an accidental toggle here.
    pub(in super::super) fn keep_node(&mut self, member: Uuid) -> Vec<Effect> {
        if self.node_is_kept(member) {
            return vec![Effect::Redraw];
        }
        let changed = self
            .graph_runtimes
            .graph_containing_member(member)
            .and_then(|graph| self.graph_runtimes.canvas_mut(graph))
            .is_some_and(|canvas| {
                crate::surface_controls::set(
                    canvas,
                    member,
                    crate::surface_controls::Control::Keep,
                    true,
                )
            });
        if !changed {
            return vec![Effect::Redraw];
        }
        self.events.push(AppEvent::NodeKept(member));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(crate) fn set_link_preview(&mut self, preview: Option<String>) -> bool {
        if self.link_preview == preview {
            false
        } else {
            self.link_preview = preview;
            true
        }
    }

    pub(in super::super) fn delete_focused_node(&mut self) -> Vec<Effect> {
        // Build the bin record off the LIVING node (identity, url,
        // title, tags — everything recovery restores), then drop the
        // node and reap what hung off it: the live content session
        // and any workbench tile. The record stages through the bin
        // port (Effect::RecordDeleted); the actor answers with the
        // refreshed list, so `removed` mirrors the store, never a
        // hand-kept copy.
        let record = self.graph_runtimes.focused_member().and_then(|m| {
            let graph = self.graph_runtimes.graph();
            let (key, node) = graph.get_node_by_id(m)?;
            let title = node.title.trim();
            // The node's whole character rides the tombstone: its
            // borne world (by id) and its facet bundle, so recovery
            // restores residency/arrangement/web state, not just
            // identity.
            let facets = self.graph_runtimes.facets().facets_of(&m).map(|f| {
                serde_json::Value::Object(
                    f.iter()
                        .map(|(id, value)| (id.as_str().to_string(), value.clone()))
                        .collect(),
                )
            });
            Some(crate::action::RemovedRecord {
                node_id: node.id,
                url: node.url().to_string(),
                title: (!title.is_empty() && title != node.url()).then(|| title.to_string()),
                tags: crate::content_tags::content_tags(graph, key)
                    .into_iter()
                    .collect(),
                deleted_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                nested: node.nested.as_ref().map(|log| log.as_str().to_string()),
                facets,
            })
        });
        let Some(record) = record else {
            return vec![Effect::Redraw];
        };
        let feed_entries = self.feeds.entry_members(record.node_id);
        // Archive-never-orphan: the world's file moves to the archive
        // slot BEFORE the bearing node leaves; a failed archive
        // aborts the delete (the node stays, nothing is lost).
        if let Some(log_id) = &record.nested
            && let Err(err) = crate::denizen::archive_world(&self.session_dir(), log_id)
        {
            tracing::warn!(%err, log_id, "world archive failed; delete aborted");
            return vec![Effect::Redraw];
        }
        let Some(member) = self.graph_runtimes.remove_focused() else {
            // The node did not leave after all: put the world back.
            if let Some(log_id) = &record.nested {
                let _ = crate::denizen::unarchive_world(&self.session_dir(), log_id);
            }
            return vec![Effect::Redraw];
        };
        self.feeds.forget_member(member);
        for entry in feed_entries {
            if let Some(graph) = self.graph_runtimes.graph_containing_member(entry)
                && let Some(canvas) = self.graph_runtimes.canvas_mut(graph)
            {
                let (_, _, unread) = self.feeds.surface_flags(entry);
                crate::surface_controls::set(
                    canvas,
                    entry,
                    crate::surface_controls::Control::Unread,
                    unread,
                );
            }
        }
        // The record is the archive now: the live facets go, and a
        // participant's runtime entry goes with its node.
        if self.denizens.residents.remove(&member).is_some() {
            let sdir = self.session_dir();
            self.denizens = crate::denizen::rebuild(
                self.graph_runtimes.facets(),
                self.graph_runtimes.graph(),
                &sdir,
                self.personae_root(),
            );
        }
        for runtime in self.forme_runtimes.iter_mut() {
            runtime.workbench.close_tile(member);
        }
        self.content.forget_node(member);
        self.events.push(AppEvent::NodeRemoved(record.url.clone()));
        vec![
            Effect::RecordDeleted { record },
            Effect::CloseContent { node: member },
            Effect::SaveSession,
            Effect::Redraw,
        ]
    }

    pub(in super::super) fn recover_deleted_node(&mut self, id: Uuid) -> Vec<Effect> {
        // Recover from the bin mirror BY IDENTITY: the node re-mints
        // under its ORIGINAL id with its recorded title/tags (the
        // canvas guards idempotency), gets selected + centered, joins
        // the visit history, and refetches. The bin record stays in
        // the store (append-only until athanor's pass); the Trail's
        // Removed section derives it away because the node is present
        // again.
        let Some(record) = self.removed.iter().find(|r| r.node_id == id).cloned() else {
            return vec![Effect::Redraw];
        };
        // Unqualified old recycle labels are evidence, not content tag writes.
        let descriptive_tags: Vec<String> = record
            .tags
            .iter()
            .filter(|label| !crate::surface_controls::is_legacy_label(label))
            .cloned()
            .collect();
        let member = self.graph_runtimes.recover_node(
            record.node_id,
            &record.url,
            record.title.as_deref(),
            &descriptive_tags,
        );
        // Residency came back; its standing subscriptions have to come with
        // it, or a behavior silently stops waking after a reload.
        (
            self.watches,
            self.app_watches,
            self.time_watches,
            self.deadbands,
        ) = crate::denizen::load_watches(&self.session_dir());
        // Restore the node's character from the tombstone: the facet
        // bundle whole, then the borne world (file back to the live
        // slot, pointer re-borne through the spine), then the participant
        // runtime so a recovered resident resides again.
        if let Some(serde_json::Value::Object(map)) = &record.facets {
            for (facet_id, value) in map {
                if let Err(error) = self.graph_runtimes.edit_facets(|facets| {
                    facets.set(
                        member,
                        chartulary::FacetId::new(facet_id.as_str()),
                        value.clone(),
                        &chartulary::AcceptAll,
                    )
                }) {
                    tracing::warn!(%member, %error, "could not restore a retained node facet");
                }
            }
        }
        if let Some(graph) = self.graph_runtimes.graph_containing_member(member)
            && let Some(canvas) = self.graph_runtimes.canvas_mut(graph)
        {
            crate::surface_controls::preserve_recycle_labels(canvas, member, &record.tags);
        }
        self.reconcile_feed_tags();
        if let Some(log_id) = &record.nested {
            let sdir = self.session_dir();
            if let Err(err) = crate::denizen::unarchive_world(&sdir, log_id) {
                tracing::warn!(%err, log_id, "world unarchive failed; recovering empty");
            }
            let _ = self.graph_runtimes.set_node_nested_for(
                member,
                Some(mere::kernel::graph::LogId::new(log_id.clone())),
            );
            self.denizens = crate::denizen::rebuild(
                self.graph_runtimes.facets(),
                self.graph_runtimes.graph(),
                &sdir,
                self.personae_root(),
            );
            // Residency came back; its standing subscriptions come with it, or
            // a recovered behavior silently stops waking.
            (
                self.watches,
                self.app_watches,
                self.time_watches,
                self.deadbands,
            ) = crate::denizen::load_watches(&sdir);
        }
        self.graph_runtimes.center_on_selected();
        self.history.visit(record.url.clone());
        self.events
            .push(AppEvent::NodeRecovered(record.url.clone()));
        let mut effects = vec![Effect::SaveSession, Effect::Redraw];
        if crate::browse::is_fetchable(&record.url) {
            effects.push(self.fetch_page_effect(member, record.url.clone(), record.url.clone()));
        }
        effects
    }

    pub(in super::super) fn set_viewer_override(
        &mut self,
        member: Uuid,
        viewer: Option<String>,
    ) -> Vec<Effect> {
        let viewer = super::super::canonical_viewer_override(viewer);
        self.browser.entry(member).viewer_override = viewer.clone();
        self.events.push(AppEvent::ViewerChanged {
            node: member,
            viewer: viewer.clone().unwrap_or_else(|| "auto".to_string()),
        });
        let mut effects = Vec::new();
        // Live, in-flight or failed content respawns through the new route.
        // Choosing a valid viewer also recovers a failed saved engine pin.
        if matches!(
            self.content.get(member),
            Some(
                crate::content::NodeContent::Live
                    | crate::content::NodeContent::Requested
                    | crate::content::NodeContent::Failed(_)
            )
        ) && let Some(url) = self
            .graph_runtimes
            .graph()
            .nodes()
            .find(|(_, n)| n.id == member)
            .map(|(_, n)| n.url().to_string())
        {
            self.content.note_requested(member);
            self.events.push(AppEvent::ContentState {
                node: member,
                state: "requested".to_string(),
            });
            effects.push(Effect::CloseContent { node: member });
            effects.push(Effect::SpawnContent { node: member, url });
        }
        effects.push(Effect::SaveSession);
        effects.push(Effect::Redraw);
        effects
    }

    /// Whether `member` offers page zoom right now: live content whose engine
    /// reports the control. The palette rows and the ctrl+wheel gesture ask
    /// this same question; the arms below ask the refusing version.
    pub(crate) fn page_zoom_offered(&self, member: Uuid) -> bool {
        matches!(
            self.content.get(member),
            Some(crate::content::NodeContent::Live)
        ) && self.content.offers(member, inker::webcue::Feature::Zoom)
    }

    /// One node's requested scale, defaulting an unset sidecar entry.
    pub(crate) fn requested_page_scale(&self, member: Uuid) -> f32 {
        self.browser
            .get(member)
            .and_then(|state| state.page_scale)
            .unwrap_or(super::super::PAGE_ZOOM_DEFAULT)
    }

    pub(in super::super) fn page_zoom_in(&mut self, member: Uuid) -> Vec<Effect> {
        if !self.refuse_unzoomable(member) {
            return vec![Effect::Redraw];
        }
        match PAGE_ZOOM_LADDER
            .iter()
            .copied()
            .find(|rung| *rung > self.requested_page_scale(member) + PAGE_ZOOM_RUNG_EPSILON)
        {
            Some(rung) => self.set_page_scale(member, Some(rung)),
            None => {
                self.note_page_zoom_refusal("already at the ladder's largest scale");
                vec![Effect::Redraw]
            },
        }
    }

    pub(in super::super) fn page_zoom_out(&mut self, member: Uuid) -> Vec<Effect> {
        if !self.refuse_unzoomable(member) {
            return vec![Effect::Redraw];
        }
        match PAGE_ZOOM_LADDER
            .iter()
            .copied()
            .rev()
            .find(|rung| *rung < self.requested_page_scale(member) - PAGE_ZOOM_RUNG_EPSILON)
        {
            Some(rung) => self.set_page_scale(member, Some(rung)),
            None => {
                self.note_page_zoom_refusal("already at the ladder's smallest scale");
                vec![Effect::Redraw]
            },
        }
    }

    pub(in super::super) fn page_zoom_reset(&mut self, member: Uuid) -> Vec<Effect> {
        if !self.refuse_unzoomable(member) {
            return vec![Effect::Redraw];
        }
        self.set_page_scale(member, None)
    }

    /// Write `member`'s REQUESTED page scale and ask its live surface to show
    /// it. The two stay separate on purpose: the engine quantizes and bounds
    /// what it applies, and never writes back over what was asked for.
    pub(super) fn set_page_scale(&mut self, member: Uuid, scale: Option<f32>) -> Vec<Effect> {
        self.browser.entry(member).page_scale = scale;
        self.events.push(AppEvent::PageZoomChanged {
            node: member,
            percent: super::super::page_zoom_percent(scale),
        });
        vec![
            Effect::ScaleContent {
                node: member,
                scale: scale.unwrap_or(super::super::PAGE_ZOOM_DEFAULT),
            },
            Effect::SaveSession,
            Effect::Redraw,
        ]
    }
}
