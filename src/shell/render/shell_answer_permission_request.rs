// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Answer a prompt after host UI or automation has made the decision.
    /// The backend is answered first; only successful answers enter retained
    /// profile policy.
    pub fn answer_permission_request(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        answer: inker::PermissionAnswer,
        retention: crate::web_policy::PermissionRetention,
    ) -> Result<(), String> {
        if self.web_policy.permission_request(node, id).is_none() {
            return Err("permission request is no longer pending".into());
        }
        self.answer_surface_permission(node, id, answer)?;
        let result = self
            .web_policy
            .complete_permission(node, id, answer, retention)
            .map_err(|error| error.to_string());
        self.project_policy_summary(node);
        result
    }

    /// Answer a challenge from host UI or a credential provider. Remembered
    /// credentials remain process-memory values and never enter the profile
    /// JSON or graph facets.
    pub fn answer_authentication_request(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        answer: inker::HttpAuthenticationAnswer,
        remember_for_process: bool,
    ) -> Result<(), String> {
        if self.web_policy.authentication_challenge(node, id).is_none() {
            return Err("authentication request is no longer pending".into());
        }
        self.answer_surface_authentication(node, id, &answer)?;
        self.web_policy
            .complete_authentication(node, id, &answer, remember_for_process)
            .map_err(|error| error.to_string())?;
        self.project_policy_summary(node);
        Ok(())
    }

    pub(super) fn expire_user_agent_requests(&mut self) {
        for expired in self.web_policy.expire(std::time::Instant::now()) {
            match expired {
                crate::web_policy::PendingRequest::Permission { node, id } => {
                    if let Err(error) =
                        self.answer_surface_permission(node, id, inker::PermissionAnswer::Dismiss)
                    {
                        tracing::warn!(%node, request_id = id.get(), %error, "timed-out permission request could not be dismissed");
                    }
                    self.project_policy_summary(node);
                    let effects =
                        self.app
                            .apply_update(crate::action::Update::UserAgentRequestWithdrawn {
                                request: crate::user_agent_decision::UserAgentRequestKey::new(
                                    node, id,
                                ),
                                kind: "permission",
                                reason: "timed-out",
                            });
                    self.run_effects(effects);
                },
                crate::web_policy::PendingRequest::Authentication { node, id } => {
                    if let Err(error) = self.answer_surface_authentication(
                        node,
                        id,
                        &inker::HttpAuthenticationAnswer::Cancel,
                    ) {
                        tracing::warn!(%node, request_id = id.get(), %error, "timed-out authentication request could not be cancelled");
                    }
                    self.project_policy_summary(node);
                    let effects =
                        self.app
                            .apply_update(crate::action::Update::UserAgentRequestWithdrawn {
                                request: crate::user_agent_decision::UserAgentRequestKey::new(
                                    node, id,
                                ),
                                kind: "authentication",
                                reason: "timed-out",
                            });
                    self.run_effects(effects);
                },
            }
        }
        self.sync_ime_allowed();
    }

    pub(in super::super) fn withdraw_user_agent_requests(
        &mut self,
        node: uuid::Uuid,
        reason: &'static str,
    ) {
        let withdrawn = self.web_policy.withdraw_node(node, reason);
        for request in withdrawn {
            let (id, kind) = match request {
                crate::web_policy::PendingRequest::Permission { id, .. } => {
                    if let Err(error) =
                        self.answer_surface_permission(node, id, inker::PermissionAnswer::Dismiss)
                    {
                        tracing::debug!(%node, request_id = id.get(), %error, "withdrawn permission callback was already unavailable");
                    }
                    (id, "permission")
                },
                crate::web_policy::PendingRequest::Authentication { id, .. } => {
                    if let Err(error) = self.answer_surface_authentication(
                        node,
                        id,
                        &inker::HttpAuthenticationAnswer::Cancel,
                    ) {
                        tracing::debug!(%node, request_id = id.get(), %error, "withdrawn authentication callback was already unavailable");
                    }
                    (id, "authentication")
                },
            };
            let effects = self
                .app
                .apply_update(crate::action::Update::UserAgentRequestWithdrawn {
                    request: crate::user_agent_decision::UserAgentRequestKey::new(node, id),
                    kind,
                    reason,
                });
            self.run_effects(effects);
        }
        if !self.web_policy.summary(node).permissions.is_empty()
            || !self.web_policy.summary(node).authentication.is_empty()
        {
            self.project_policy_summary(node);
        }
        self.sync_ime_allowed();
    }

    pub(in super::super) fn withdraw_all_user_agent_requests(&mut self, reason: &'static str) {
        let nodes = self.web_policy.pending_nodes();
        for node in nodes {
            self.withdraw_user_agent_requests(node, reason);
        }
    }

    pub(super) fn answer_surface_permission(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        answer: inker::PermissionAnswer,
    ) -> Result<(), String> {
        let producer = self
            .content_sessions
            .surface_mut(&node)
            .ok_or_else(|| "requesting surface no longer exists".to_string())?;
        let web = producer
            .as_web_surface()
            .ok_or_else(|| "requesting surface has no web control plane".to_string())?;
        web.answer_permission(id, answer)
            .map_err(|error| error.to_string())
    }

    pub(super) fn answer_surface_authentication(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        answer: &inker::HttpAuthenticationAnswer,
    ) -> Result<(), String> {
        let producer = self
            .content_sessions
            .surface_mut(&node)
            .ok_or_else(|| "requesting surface no longer exists".to_string())?;
        let web = producer
            .as_web_surface()
            .ok_or_else(|| "requesting surface has no web control plane".to_string())?;
        web.answer_http_authentication(id, answer)
            .map_err(|error| error.to_string())
    }

    pub(super) fn project_policy_summary(&mut self, node: uuid::Uuid) {
        let value = self.web_policy.facet_value(node);
        if let Err(error) = self.app.graph_runtimes.edit_facets(|facets| {
            facets.set(
                node,
                chartulary::FacetId::new(crate::web_policy::USER_AGENT_POLICY_FACET),
                value,
                &chartulary::AcceptAll,
            )
        }) {
            tracing::warn!(%node, %error, "could not project user-agent policy summary facet");
        }
    }

    /// One pane's scene by kind, at `(rw, rh)`, through the shared retained
    /// runners — used by the primary render AND every lens window (rung 7
    /// depth: windows are pane hosts). The runner being shared is what makes
    /// tear-out identity-preserving in the surface-compositor shape: the pane
    /// keeps its DOM, widget state, and scroll because the runner never moves.
    /// Trail renders real rows off graph truth (slice D); kinds without real
    /// content are labeled placeholders (slice C), honestly.
    pub(in super::super) fn pane_scene_by_kind(
        &mut self,
        pane_id: crate::panes::PaneId,
        content: Option<&PaneContent>,
        rw: u32,
        rh: u32,
    ) -> Scene {
        // The pane's rect is physical; it is laid out at `physical / scale`
        // and its scene rasterized at `scale`.
        let scale = self.app.pane_layout_scale(pane_id);
        let (pw, ph) = (rw as f32 / scale, rh as f32 / scale);
        let (lw, lh) = ((pw.round() as u32).max(1), (ph.round() as u32).max(1));
        if let Some(spec) = self.contributed_pane_spec(pane_id, content) {
            return match self
                .renderers
                .contributed
                .resolve(&spec, &self.surface_providers)
            {
                Ok(pane) => pane.scene(rw, rh, scale),
                Err(error) => {
                    crate::ui::pane_scene(&format!("Surface admission failed: {error}"), lw, lh)
                },
            };
        }
        let dom = self.renderers.dom.clone();
        // A legacy blueprint rasterizes its scoped owner without replacing
        // its runner. Joining a Workspace reparents this same owner.
        let mount = || Some(cambium::RetainedMount::new(dom.clone()));
        match content {
            Some(PaneContent::Trail) => {
                let pane = self
                    .renderers
                    .trail
                    .entry(pane_id)
                    .or_insert_with(|| crate::trail_pane::TrailPane::new_in(mount()));
                pane.sync(&self.app, pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Roster) => {
                // The retained cambium grid: refresh it from graph truth at
                // the pane's size, then draw its DOM.
                let grid = self
                    .renderers
                    .roster
                    .entry(pane_id)
                    .or_insert_with(|| crate::cambium_pane::RosterGrid::new_in(mount()));
                grid.sync(&self.app, pane_id, pw, ph);
                grid.scene(rw, rh, scale)
            },
            Some(PaneContent::Gloss(cfg)) => {
                // The minimap: the swatch's custom-paint leaf renders through
                // the pane's registry (the leaf pipeline). Its composed
                // sections come from THIS LEAF's config, resolved against the
                // section registry (unknown ids are ignored, so a config from
                // a newer build degrades instead of failing).
                let providers = crate::sections::resolve(&cfg.sections);
                let pane = self.renderers.gloss.entry(pane_id).or_insert_with(|| {
                    crate::swatch_pane::SwatchPane::new_in(
                        mount(),
                        crate::swatch_pane::GLOSS_MINIMAP,
                    )
                });
                pane.set_sections(providers);
                pane.sync(&self.app, pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Inspector) => {
                let admission = self
                    .renderers
                    .inspector
                    .entry(pane_id)
                    .or_insert_with(|| crate::inspector_pane::InspectorPane::new_in(mount()))
                    .admission();
                let (clip_target, clip_offer, clip_status) =
                    self.research_clip_offer(pane_id, admission);
                let pane = self.renderers.inspector.get_mut(&pane_id).unwrap();
                pane.sync(
                    &self.app,
                    pane_id,
                    pw,
                    ph,
                    clip_target.as_deref(),
                    clip_offer,
                    &clip_status,
                );
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Workbench) => {
                // The tiling's furniture: tab strips + cell bodies. Tile
                // documents composite as their own surfaces in the PRIMARY
                // plan; in a lens the furniture shows and tile compositing is
                // a named follow-on.
                let pane = self
                    .renderers
                    .workbench
                    .entry(pane_id)
                    .or_insert_with(|| crate::workbench_pane::WorkbenchPane::new_in(mount()));
                pane.sync(&self.app, pane_id, rw as f32, rh as f32);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::TRANSCRIPT =>
            {
                // Reads the shell ledger the omnibar already writes; it
                // derives nothing of its own.
                let pane = self
                    .renderers
                    .transcript
                    .entry(pane_id)
                    .or_insert_with(|| crate::transcript_pane::TranscriptPane::new_in(mount()));
                pane.sync(self.app.shell_transcript(), pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Registered(kind)) if kind.as_str() == crate::panes::kind::ARRANGE => {
                let pane = self
                    .renderers
                    .arrange
                    .entry(pane_id)
                    .or_insert_with(|| crate::arrange_pane::ArrangePane::new_in(mount()));
                pane.sync_for_pane(&self.app, pane_id, pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::SETTINGS =>
            {
                let live_settings = self.live_settings.clone();
                let pane = self.renderers.settings.entry(pane_id).or_insert_with(|| {
                    crate::settings_pane::SettingsPane::new_in(
                        mount(),
                        self.app.data_root.clone(),
                        Some(live_settings),
                    )
                });
                pane.sync(pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::PUBLISHING =>
            {
                let service = self.publish_service.clone();
                let pane =
                    self.renderers.publish.entry(pane_id).or_insert_with(|| {
                        crate::publish_pane::PublishPane::new_in(mount(), service)
                    });
                pane.sync(pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::SHARED_KNOT =>
            {
                let service = self.shared_knot_service.clone();
                let pane = self
                    .renderers
                    .shared_knot
                    .entry(pane_id)
                    .or_insert_with(|| {
                        crate::share_reader_pane::SharedKnotPane::new_in(mount(), service)
                    });
                pane.sync(pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
            {
                let service = self.device_receipts_service.clone();
                let pane = self
                    .renderers
                    .device_receipts
                    .entry(pane_id)
                    .or_insert_with(|| {
                        crate::device_receipts_pane::DeviceReceiptsPane::new_in(mount(), service)
                    });
                pane.sync(pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION =>
            {
                let pane = self
                    .renderers
                    .frozen_projection
                    .entry(pane_id)
                    .or_insert_with(|| {
                        crate::frozen_projection_pane::FrozenProjectionPane::new_in(mount())
                    });
                pane.sync(&self.app, pw, ph);
                pane.scene(rw, rh, scale)
            },
            Some(PaneContent::Overmap(cfg)) => {
                // The switcher as a graph view (overmap O1): sessions as
                // container nodes, fork lineage as edges, on the shared
                // custom-paint swatch. It composes sections the same way the
                // Gloss does, off ITS OWN leaf: one renderer, one config shape,
                // so the second host cost a resolve and a setter.
                let providers = crate::sections::resolve(&cfg.sections);
                let pane = self.renderers.overmap.entry(pane_id).or_insert_with(|| {
                    crate::swatch_pane::SwatchPane::new_in(
                        mount(),
                        crate::swatch_pane::OVERMAP_LINEAGE,
                    )
                });
                pane.set_sections(providers);
                pane.sync(&self.app, pw, ph);
                pane.scene(rw, rh, scale)
            },
            other => {
                let label = other
                    .map(|c| match &c {
                        PaneContent::Registered(kind) => {
                            crate::unavailable_pane::contributed_label(kind)
                                .map(str::to_string)
                                .unwrap_or_else(|| pane_display_label(c))
                        },
                        _ => pane_display_label(c),
                    })
                    .unwrap_or_default();
                let pane = self
                    .renderers
                    .placeholder
                    .entry(pane_id)
                    .or_insert_with(|| crate::inline_pane::InlinePane::new(dom.clone()));
                pane.sync(label, lw, lh);
                pane.scene(rw, rh, scale)
            },
        }
    }

    pub(in super::super) fn contributed_pane_spec(
        &self,
        pane_id: crate::panes::PaneId,
        content: Option<&PaneContent>,
    ) -> Option<crate::panes::PaneSpec> {
        let PaneContent::Registered(kind) = content? else {
            return None;
        };
        let spec = self
            .app
            .space(self.app.space_of(pane_id)?)?
            .outer_workspace()
            .ok()
            .and_then(|frame| frame.descriptors.get(&pane_id.0).cloned())
            .and_then(|value| serde_json::from_value::<crate::panes::PaneSpec>(value).ok())
            .or_else(|| {
                self.app
                    .blueprint_space(self.app.space_of(pane_id)?)?
                    .pane(pane_id)
                    .cloned()
            })?;
        (&spec.kind == kind && self.surface_providers.contains_pane_kind(&spec.kind))
            .then(|| spec.clone())
    }
}
