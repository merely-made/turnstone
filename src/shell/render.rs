//! Rendering: each surface's scene, the layered present, and the self-capture
//! that writes a receipt's PNG.
//!
//! Two passes on purpose. The first is mutable (a content session paints its
//! own frame); the second rasterizes and composes immutably, so a session's
//! mutable borrow never overlaps the host's. The capture path composes the
//! same layer list the presented frame did, so a receipt shows what was shown.

use std::path::Path;

use genet_winit_host::SurfaceHost;
use image::ImageEncoder;
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, Scene};

use crate::action::Action;
use crate::panes::PaneContent;
use crate::surface::SurfaceKind;

use winit::dpi::{PhysicalPosition, PhysicalSize};

use super::{CompositeLayer, PlannedLayer, PlannedScene, Shell, pane_display_label};

impl Shell {
    /// Drain each producer's one host-facing stream before building a frame.
    /// The bounded batch prevents a faulty producer from monopolising the UI
    /// thread; another redraw continues the drain.
    fn drain_surface_web_events(&mut self) {
        const MAX_EVENTS_PER_SURFACE: usize = 256;
        let mut pending = Vec::new();
        for (&node, producer) in &mut self.surface_producers {
            let Some(web) = producer.as_web_surface() else {
                continue;
            };
            for _ in 0..MAX_EVENTS_PER_SURFACE {
                let Some(event) = web.poll_web_event() else {
                    break;
                };
                pending.push((node, event));
            }
        }
        for (node, event) in pending {
            self.consume_surface_web_event(node, event);
        }
        self.expire_user_agent_requests();
    }

    fn consume_surface_web_event(&mut self, node: uuid::Uuid, event: inker::WebSurfaceEvent) {
        match event {
            inker::WebSurfaceEvent::Navigation(inker::NavigationEvent::Committed { url })
            | inker::WebSurfaceEvent::AddressChanged { url } => {
                self.act(Action::ContentNavigationCommitted { member: node, url });
            }
            inker::WebSurfaceEvent::Navigation(inker::NavigationEvent::Finished {
                title: Some(title),
                ..
            })
            | inker::WebSurfaceEvent::TitleChanged { title } => {
                self.act(Action::ContentTitleChanged {
                    member: node,
                    title,
                });
            }
            inker::WebSurfaceEvent::NewWindowRequested { url } => {
                self.app
                    .note(crate::observe::AppEvent::AuxiliaryNavigableRequested { node, url });
                // This event observes a request rather than lowering a graph
                // mutation. Publish it now so its order against adjacent
                // producer events is retained.
                self.run_effects(Vec::new());
            }
            inker::WebSurfaceEvent::PageDragStarted {
                data_transfer,
                position,
            } => {
                let types = data_transfer
                    .items
                    .iter()
                    .map(|item| match item {
                        inker::DataTransferItem::String { mime_type, .. } => mime_type.clone(),
                        inker::DataTransferItem::File { mime_type, .. } if mime_type.is_empty() => {
                            "file".into()
                        }
                        inker::DataTransferItem::File { mime_type, .. } => {
                            format!("file:{mime_type}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                self.app
                    .note(crate::observe::AppEvent::PageDragRequested { node, types });
                // Winit has no native API that can start an OS drag from a
                // windowless CEF surface. Answer CEF explicitly so its source
                // does not remain in a stuck drag state; capability reporting
                // keeps this path Partial until a toolkit drag carrier lands.
                if let Some(producer) = self.surface_producers.get_mut(&node)
                    && let Err(error) =
                        producer.finish_drag_source(position, inker::DragOperationSet::NONE)
                {
                    tracing::warn!(%node, %error, "surface page drag cancellation failed");
                }
                self.run_effects(Vec::new());
            }
            inker::WebSurfaceEvent::PermissionRequested(request) => {
                let disposition = self.web_policy.receive_permission(
                    node,
                    request.clone(),
                    std::time::Instant::now(),
                );
                self.project_policy_summary(node);
                match disposition {
                    crate::web_policy::PermissionDisposition::Answer(answer) => {
                        if let Err(error) = self.answer_surface_permission(node, request.id, answer)
                        {
                            tracing::warn!(%node, request_id = request.id.get(), %error, "stored permission decision could not be applied");
                        }
                    }
                    crate::web_policy::PermissionDisposition::Pending => {
                        self.app
                            .note(crate::observe::AppEvent::PermissionRequested {
                                node,
                                id: request.id,
                                origin: request.origin,
                                descriptors: request.descriptors,
                            });
                        self.run_effects(Vec::new());
                    }
                }
            }
            inker::WebSurfaceEvent::AuthenticationRequested(challenge) => {
                let disposition = self.web_policy.receive_authentication(
                    node,
                    challenge.clone(),
                    std::time::Instant::now(),
                );
                self.project_policy_summary(node);
                match disposition {
                    crate::web_policy::AuthenticationDisposition::Answer(answer) => {
                        if let Err(error) =
                            self.answer_surface_authentication(node, challenge.id, &answer)
                        {
                            tracing::warn!(%node, request_id = challenge.id.get(), %error, "credential-provider answer could not be applied");
                        }
                    }
                    crate::web_policy::AuthenticationDisposition::Pending => {
                        self.app
                            .note(crate::observe::AppEvent::AuthenticationRequested {
                                node,
                                id: challenge.id,
                                host: challenge.protection_space.host,
                                realm: challenge.protection_space.realm,
                                scheme: challenge.protection_space.scheme,
                            });
                        self.run_effects(Vec::new());
                    }
                }
            }
            inker::WebSurfaceEvent::Navigation(inker::NavigationEvent::Failed { url, reason }) => {
                tracing::warn!(%node, %url, %reason, "surface navigation failed");
            }
            inker::WebSurfaceEvent::ProcessCrashed { reason } => {
                tracing::warn!(%node, %reason, "surface content process crashed");
            }
            inker::WebSurfaceEvent::BackendDiagnostic { severity, message } => {
                tracing::debug!(%node, %severity, %message, "surface backend diagnostic");
            }
            other => {
                // S0 makes every producer event observable through one stream.
                // Later gates give policy/download/representation events typed
                // request ids and host actions; until then they remain loud in
                // diagnostics rather than disappearing in a filtered poller.
                tracing::debug!(%node, event = ?other, "surface event awaits host projection");
            }
        }
    }

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

    fn expire_user_agent_requests(&mut self) {
        for expired in self.web_policy.expire(std::time::Instant::now()) {
            match expired {
                crate::web_policy::ExpiredRequest::Permission { node, id } => {
                    if let Err(error) =
                        self.answer_surface_permission(node, id, inker::PermissionAnswer::Dismiss)
                    {
                        tracing::warn!(%node, request_id = id.get(), %error, "timed-out permission request could not be dismissed");
                    }
                    self.project_policy_summary(node);
                }
                crate::web_policy::ExpiredRequest::Authentication { node, id } => {
                    if let Err(error) = self.answer_surface_authentication(
                        node,
                        id,
                        &inker::HttpAuthenticationAnswer::Cancel,
                    ) {
                        tracing::warn!(%node, request_id = id.get(), %error, "timed-out authentication request could not be cancelled");
                    }
                    self.project_policy_summary(node);
                }
            }
        }
    }

    fn answer_surface_permission(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        answer: inker::PermissionAnswer,
    ) -> Result<(), String> {
        let producer = self
            .surface_producers
            .get_mut(&node)
            .ok_or_else(|| "requesting surface no longer exists".to_string())?;
        let web = producer
            .as_web_surface()
            .ok_or_else(|| "requesting surface has no web control plane".to_string())?;
        web.answer_permission(id, answer)
            .map_err(|error| error.to_string())
    }

    fn answer_surface_authentication(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        answer: &inker::HttpAuthenticationAnswer,
    ) -> Result<(), String> {
        let producer = self
            .surface_producers
            .get_mut(&node)
            .ok_or_else(|| "requesting surface no longer exists".to_string())?;
        let web = producer
            .as_web_surface()
            .ok_or_else(|| "requesting surface has no web control plane".to_string())?;
        web.answer_http_authentication(id, answer)
            .map_err(|error| error.to_string())
    }

    fn project_policy_summary(&mut self, node: uuid::Uuid) {
        let value = self.web_policy.facet_value(node);
        if let Err(error) = self.app.graph_runtimes.facets_mut().set(
            node,
            chartulary::FacetId::new(crate::web_policy::USER_AGENT_POLICY_FACET),
            value,
            &chartulary::AcceptAll,
        ) {
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
    pub(super) fn pane_scene_by_kind(
        &mut self,
        pane_id: crate::panes::PaneId,
        content: Option<&PaneContent>,
        rw: u32,
        rh: u32,
    ) -> Scene {
        match content {
            Some(PaneContent::Trail) => {
                let pane = self
                    .renderers
                    .trail
                    .entry(pane_id)
                    .or_insert_with(crate::trail_pane::TrailPane::new);
                pane.sync(&self.app, rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Roster) => {
                // The retained cambium grid: refresh it from graph truth at
                // the pane's size, then draw its DOM.
                let grid = self
                    .renderers
                    .roster
                    .entry(pane_id)
                    .or_insert_with(crate::cambium_pane::RosterGrid::new);
                grid.sync(&self.app, pane_id, rw as f32, rh as f32);
                grid.scene(rw, rh)
            }
            Some(PaneContent::Gloss(cfg)) => {
                // The minimap: the swatch's custom-paint leaf renders through
                // the pane's registry (the leaf pipeline). Its composed
                // sections come from THIS LEAF's config, resolved against the
                // section registry (unknown ids are ignored, so a config from
                // a newer build degrades instead of failing).
                let providers = crate::sections::resolve(&cfg.sections);
                let pane = self.renderers.gloss.entry(pane_id).or_insert_with(|| {
                    crate::swatch_pane::SwatchPane::new(crate::swatch_pane::GLOSS_MINIMAP)
                });
                pane.set_sections(providers);
                pane.sync(&self.app, rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Inspector) => {
                let clip_source_available = self
                    .app
                    .follower_context(pane_id)
                    .and_then(|context| context.member)
                    .and_then(|member| self.content_sessions.get(&member))
                    .and_then(|session| session.clip())
                    .is_some();
                let clip_target = self
                    .knot_clip
                    .as_ref()
                    .map(|handle| handle.target().to_string());
                let clip_status = self
                    .knot_clip
                    .as_ref()
                    .map(|handle| handle.status().label())
                    .unwrap_or_else(|| "unconfigured".into());
                let pane = self
                    .renderers
                    .inspector
                    .entry(pane_id)
                    .or_insert_with(crate::inspector_pane::InspectorPane::new);
                pane.sync(
                    &self.app,
                    pane_id,
                    rw as f32,
                    rh as f32,
                    clip_target.as_deref(),
                    clip_source_available,
                    &clip_status,
                );
                pane.scene(rw, rh)
            }
            Some(PaneContent::Workbench) => {
                // The tiling's furniture: tab strips + cell bodies. Tile
                // documents composite as their own surfaces in the PRIMARY
                // plan; in a lens the furniture shows and tile compositing is
                // a named follow-on.
                let pane = self
                    .renderers
                    .workbench
                    .entry(pane_id)
                    .or_insert_with(crate::workbench_pane::WorkbenchPane::new);
                pane.sync(&self.app, pane_id, rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Apparatus) => {
                // The graph-object facet analyzer's first rows: the viewer
                // control (radio over the registered lanes).
                let pane = self
                    .renderers
                    .apparatus
                    .entry(pane_id)
                    .or_insert_with(crate::apparatus_pane::ApparatusPane::new);
                pane.sync(&self.app, rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::TRANSCRIPT =>
            {
                // Reads the shell ledger the omnibar already writes; it
                // derives nothing of its own.
                let pane = self
                    .renderers
                    .transcript
                    .entry(pane_id)
                    .or_insert_with(crate::transcript_pane::TranscriptPane::new);
                pane.sync(self.app.shell_transcript(), rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::SETTINGS =>
            {
                let live_settings = self.live_settings.clone();
                let pane = self.renderers.settings.entry(pane_id).or_insert_with(|| {
                    crate::settings_pane::SettingsPane::with_live_settings(
                        self.app.data_root.clone(),
                        Some(live_settings),
                    )
                });
                pane.sync(rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::PUBLISHING =>
            {
                let service = self.publish_service.clone();
                let pane = self
                    .renderers
                    .publish
                    .entry(pane_id)
                    .or_insert_with(|| crate::publish_pane::PublishPane::new(service));
                pane.sync(rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::SHARED_KNOT =>
            {
                let service = self.shared_knot_service.clone();
                let pane = self
                    .renderers
                    .shared_knot
                    .entry(pane_id)
                    .or_insert_with(|| crate::share_reader_pane::SharedKnotPane::new(service));
                pane.sync(rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Registered(kind))
                if kind.as_str() == crate::panes::kind::DEVICE_RECEIPTS =>
            {
                let service = self.device_receipts_service.clone();
                let pane = self
                    .renderers
                    .device_receipts
                    .entry(pane_id)
                    .or_insert_with(|| {
                        crate::device_receipts_pane::DeviceReceiptsPane::new(service)
                    });
                pane.sync(rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            Some(PaneContent::Overmap(cfg)) => {
                // The switcher as a graph view (overmap O1): sessions as
                // container nodes, fork lineage as edges, on the shared
                // custom-paint swatch. It composes sections the same way the
                // Gloss does, off ITS OWN leaf: one renderer, one config shape,
                // so the second host cost a resolve and a setter.
                let providers = crate::sections::resolve(&cfg.sections);
                let pane = self.renderers.overmap.entry(pane_id).or_insert_with(|| {
                    crate::swatch_pane::SwatchPane::new(crate::swatch_pane::OVERMAP_LINEAGE)
                });
                pane.set_sections(providers);
                pane.sync(&self.app, rw as f32, rh as f32);
                pane.scene(rw, rh)
            }
            other => {
                let label = other.map(|c| pane_display_label(c)).unwrap_or_default();
                crate::ui::pane_scene(&label, rw, rh)
            }
        }
    }

    /// The layered present (born minimal at rung 3, grows into the surface
    /// plan at rung 5): rasterize each surface's scene to its own texture and
    /// compose them in order onto the frame — the canvas below, the chrome
    /// layer (transparent-cleared, alpha-blended) above when the omnibar is
    /// open. Chains another redraw while the canvas is still animating.
    pub(super) fn render(&mut self) {
        if self.host.is_none() {
            return;
        }
        // Frame cost, off by default (the shell's own filter is `info`). Turn
        // it on with `RUST_LOG=turnstone::shell::render=debug` when a window
        // feels heavy: a frame is the unit that lags, and the surface count
        // beside it is what usually explains the number. This exists because
        // diagnosing a 24 ms-per-pane relayout once meant instrumenting the
        // frame loop by hand.
        let started = std::time::Instant::now();
        self.drain_surface_web_events();
        // Cursor callbacks may arrive after the move that provoked them. Poll
        // the hovered surface on frame wakes as well as immediately on input.
        self.apply_pending_surface_cursor();
        if self.poll_live_settings() {
            // A SettingsPane has already persisted the write. Refresh every
            // affected retained surface in this and any lens window now.
            self.request_redraw();
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        // Aim the IME candidate window at the caret's neighborhood, so
        // composition popups open beside the omnibar input rather than at
        // the window corner.
        if self.app.omnibar.open
            && self.app.shell_chrome_config().projects_omnibar()
            && let Some(window) = self.window.as_ref()
            && let Some((left, top)) = crate::chrome_view::chrome_position(
                &self.app.shell_chrome_config().omnibar.placement,
                w as f32,
                h as f32,
                crate::ui::CARD_W,
            )
        {
            let (pos, size) = crate::ui::ime_cursor_area_at(&self.app.omnibar, left, top);
            window.set_ime_cursor_area(
                PhysicalPosition::new(pos.0, pos.1),
                PhysicalSize::new(size.0, size.1),
            );
        }
        // The surface plan (rung 5 slice A): the ordered list of composited
        // surfaces, each with its own rect. Built by the same helper input
        // routing uses, so what a frame draws and what a pointer hits agree.
        let surfaces = self.surface_plan();
        let caption = self
            .app
            .focused_graph_pane()
            .and_then(|pane| self.app.graph_for_pane(pane))
            .and_then(|graph| self.app.graph_runtimes.canvas(graph))
            .and_then(crate::app::focused_caption);

        // Bug #2 (rung-4 debt): keep EVERY live session's clock advancing, not
        // just the framed one. Before this, a session lost focus and stopped
        // pumping, so `Live` was a lie for every non-focused node. Pumping is
        // cheap for the settled static lane and correct for future animated
        // ones; only the framed surface is rasterized below.
        let now_ms = self.epoch.elapsed().as_secs_f64() * 1000.0;
        let mut needs_redraw = false;
        for session in self.content_sessions.values_mut() {
            session.pump(now_ms);
            if !session.settled() {
                needs_redraw = true;
            }
        }

        #[cfg(all(feature = "weld", windows))]
        let (surface_device, surface_queue) = {
            let host = self.host.as_ref().expect("host checked at render entry");
            (host.device().clone(), host.queue().clone())
        };

        // Pass 1 (mutable): produce each surface's scene at ITS rect size. Kept
        // separate from rasterization so framing a content session (which
        // borrows `content_sessions` mutably) never overlaps the immutable
        // `host` borrow the second pass holds.
        let mut scenes: Vec<PlannedLayer> = Vec::with_capacity(surfaces.len());
        for surface in &surfaces {
            let rect = surface.rect;
            let (rw, rh) = (
                rect.w.round().max(1.0) as u32,
                rect.h.round().max(1.0) as u32,
            );
            let (scene, clear) = match surface.kind {
                crate::surface::SurfaceKind::Graph(pane) => {
                    // A graph surface is addressed by its pane. The app swaps
                    // this pane's camera/selection into its graph runtime for
                    // exactly this frame, then stashes it back before another
                    // pane can render the same graph.
                    let (scene, animating) = self
                        .app
                        .graph_pane_frame(pane, rw, rh)
                        .unwrap_or_else(|| (Scene::default(), false));
                    needs_redraw |= animating;
                    needs_redraw |= self.app.resolve_pending_images() > 0;
                    (scene, wgpu::Color::WHITE)
                }
                crate::surface::SurfaceKind::Content(node) => {
                    if let Some(session) = self.content_sessions.get_mut(&node) {
                        // Already pumped above; just frame it at the pane size.
                        let scene = session.frame(rw, rh);
                        (scene, wgpu::Color::WHITE)
                    } else {
                        #[cfg(all(feature = "weld", windows))]
                        if let Some(producer) = self.surface_producers.get_mut(&node) {
                            if let Err(error) = producer.resize(rw, rh) {
                                tracing::warn!(%node, %error, "surface producer resize failed");
                            }
                            match producer.acquire_frame() {
                                Ok(Some(frame)) => {
                                    let cached = self.surface_frames.entry(node).or_insert(None);
                                    if let Err(error) = super::surface_frames::update_imported_frame(
                                        cached,
                                        frame,
                                        &surface_device,
                                        &surface_queue,
                                    ) {
                                        tracing::warn!(%node, %error, "surface frame import failed");
                                    }
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    tracing::warn!(%node, %error, "surface frame acquisition failed")
                                }
                            }
                            if let Some(Some(frame)) = self.surface_frames.get(&node) {
                                scenes.push(PlannedLayer::Imported(CompositeLayer {
                                    kind: surface.kind,
                                    view: frame.view(),
                                    placement: ExternalTexturePlacement::new(rect.dest()),
                                }));
                            }
                            // CEF paints on its own thread. Keep driving its
                            // mailbox until the shell has a wake bridge.
                            needs_redraw = true;
                            continue;
                        }
                        continue;
                    }
                }
                crate::surface::SurfaceKind::Pane(id) => {
                    // The pane's scene by kind, through the SHARED retained
                    // runners (extracted so lens windows render the same
                    // panes through the same runners — the identity story).
                    let content = self.pane_content(id);
                    let scene = self.pane_scene_by_kind(id, content.as_ref(), rw, rh);
                    (scene, wgpu::Color::TRANSPARENT)
                }
                crate::surface::SurfaceKind::Divider(_) => {
                    // The band is the clear colour; nothing to draw over it.
                    (Scene::default(), crate::ui::SEAM_CLEAR)
                }
                crate::surface::SurfaceKind::Chrome => {
                    // One sync rebuilds every window's chrome projection (the
                    // one-state contract); this window paints ITS root.
                    let mut sizes = vec![(0usize, rw as f32, rh as f32)];
                    sizes.extend(
                        self.lens_windows
                            .values()
                            .map(|lens| (lens.ordinal + 1, lens.width as f32, lens.height as f32)),
                    );
                    self.chrome.sync(&self.app, &sizes);
                    let scene = self.chrome.scene(0, rw, rh);
                    (scene, wgpu::Color::TRANSPARENT)
                }
            };
            scenes.push(PlannedLayer::Scene(PlannedScene {
                id: surface.id.0,
                kind: surface.kind,
                placement: ExternalTexturePlacement::new(rect.dest()),
                dims: (rw, rh),
                scene,
                clear,
            }));
        }

        // Pass 2 (immutable): rasterize each scene keyed by its surface id (so
        // an unchanged surface reuses its tile instead of rebuilding every
        // frame) and compose the layers in order.
        let host = self.host.as_ref().unwrap();
        let layers: Vec<CompositeLayer> = scenes
            .into_iter()
            .map(|source| match source {
                PlannedLayer::Scene(scene) => {
                    let (_tex, view) = host.core().rasterize_for(
                        scene.id,
                        &scene.scene,
                        scene.dims.0,
                        scene.dims.1,
                        ColorLoad::Clear(scene.clear),
                    );
                    CompositeLayer {
                        kind: scene.kind,
                        view,
                        placement: scene.placement,
                    }
                }
                PlannedLayer::Imported(layer) => layer,
            })
            .collect();

        let Some(frame) = host.acquire() else { return };
        let target = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        for layer in &layers {
            host.renderer().compose_external_texture(
                &layer.view,
                &target,
                host.format(),
                w,
                h,
                layer.placement,
            );
        }
        // wgpu 30 moved presentation from SurfaceTexture to Queue.
        host.queue().present(frame);

        // Scenario self-capture: compose the SAME layer views this frame just
        // presented into an owned COPY_SRC target and read it back — the
        // receipt is the presented frame, not a re-rasterization (a second
        // `canvas.frame()` in the same pass produced stale, layer-dropping
        // captures). Immune to focus theft and occlusion by construction.
        if let Some(path) = self.pending_capture.take() {
            tracing::info!(
                open = self.app.omnibar.open,
                text = %self.app.omnibar.text,
                suggestions = self.app.omnibar.suggestions.len(),
                surfaces = layers.len(),
                chrome = layers
                    .iter()
                    .any(|l| matches!(l.kind, crate::surface::SurfaceKind::Chrome)),
                nodes = self.app.graph_runtimes.graph().nodes().count(),
                "capture state"
            );
            let ok = capture_composed(host, &layers, w, h, &path);
        }

        tracing::debug!(
            frame_ms = started.elapsed().as_secs_f32() * 1000.0,
            surfaces = surfaces.len(),
            w,
            h,
            "frame"
        );

        // An overlay bar mid-hold or mid-fade needs the next frame to draw it
        // one step dimmer. Without this the renderer, being change-driven,
        // would stop after the scroll and leave the bar frozen on screen --
        // an auto-hiding bar that never hides.
        if needs_redraw || self.renderers.any_bars_visible() {
            self.request_redraw();
        }
    }

    pub(super) fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        for lens in self.lens_windows.values() {
            lens.window.request_redraw();
        }
    }
}

/// Decode a dropped image file into a face-sized PNG data-URI plus its traced
/// collider hull, or `None` for a file the image decoder does not read (which
/// then becomes a node instead). Downscaled so the per-node URI stays small
/// (the face draws at ~24-120px). The hull is canvas's shared tracer (the
/// meerkat-harvest promotion), so the node collides at its picture.
pub(super) fn decode_sprite(path: &Path) -> Option<(String, Vec<(f32, f32)>)> {
    const SPRITE_MAX: u32 = 256;
    let rgba = image::open(path)
        .ok()?
        .thumbnail(SPRITE_MAX, SPRITE_MAX)
        .to_rgba8();
    let (w, h) = rgba.dimensions();
    let hull = mere::canvas::sprite_hull::trace_sprite_hull(rgba.as_raw(), w, h);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba.as_raw(), w, h, image::ExtendedColorType::Rgba8)
        .ok()?;
    use base64::Engine as _;
    Some((
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&png)
        ),
        hull,
    ))
}

/// Compose the frame's already-rasterized layers into an owned `COPY_SRC`
/// target, read the pixels back, and encode a PNG at `path`. Composes the same
/// layer list, each at its own placement, that the presented frame did, so the
/// receipt matches what was shown (occlusion and all).
pub(super) fn capture_composed(
    host: &SurfaceHost,
    layers: &[CompositeLayer],
    w: u32,
    h: u32,
    path: &Path,
) -> bool {
    let target = host.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("turnstone scenario capture"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
    for layer in layers {
        host.renderer().compose_external_texture(
            &layer.view,
            &target_view,
            wgpu::TextureFormat::Rgba8Unorm,
            w,
            h,
            layer.placement,
        );
    }
    let rgba = read_texture_rgba(host.device(), host.queue(), &target, w, h);
    if rgba.is_empty() {
        return false;
    }
    let Ok(file) = std::fs::File::create(path) else {
        return false;
    };
    image::codecs::png::PngEncoder::new(file)
        .write_image(&rgba, w, h, image::ExtendedColorType::Rgba8)
        .is_ok()
}

/// Read a texture's pixels back as tightly packed RGBA8 (empty on failure).
/// Standard wgpu readback: copy into a row-aligned buffer, map, strip the
/// per-row padding.
pub(super) fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let row_bytes = width * 4;
    let padded = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("turnstone capture readback"),
        size: padded as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("turnstone capture readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    if device.poll(wgpu::PollType::wait_indefinitely()).is_err() {
        tracing::warn!("capture readback poll failed");
        return Vec::new();
    }
    if !matches!(rx.recv(), Ok(Ok(()))) {
        tracing::warn!("capture readback map failed");
        return Vec::new();
    }
    let Ok(mapped) = slice.get_mapped_range() else {
        tracing::warn!("capture readback get_mapped_range failed");
        return Vec::new();
    };
    let mut out = Vec::with_capacity((row_bytes * height) as usize);
    for row in 0..height as usize {
        let start = row * padded as usize;
        out.extend_from_slice(&mapped[start..start + row_bytes as usize]);
    }
    out
}
