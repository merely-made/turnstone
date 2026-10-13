// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Each lens publishes to its own native adapter. Producer semantics still
//! enter through one mailbox; mirrors own only publication/replay state.
use super::{NodeSurfaces, Shell};
use crate::{
    a11y::{A11yRoute, A11yRoutes},
    surface::{FocusTarget, SurfaceKind},
    window_projection::WindowScope,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use winit::{
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);
pub(super) struct WindowA11y {
    pub adapter: accesskit_winit::Adapter,
    pub shared: super::a11y_bridge::SharedTree,
    pub actions: super::a11y_bridge::ActionQueue,
    pub replay: Arc<AtomicBool>,
    pub foreign: crate::foreign_a11y::ForeignA11y,
    pub routes: A11yRoutes,
    pub scope: WindowScope,
    frame: Option<crate::frame_controls::FrameStamp>,
    focus: Option<FocusTarget>,
    placements: Vec<(uuid::Uuid, accesskit::Rect)>,
    frames: u32,
}
impl WindowA11y {
    pub fn install(
        event_loop: &ActiveEventLoop,
        window: &Window,
        wake: winit::event_loop::EventLoopProxy<()>,
        slot: usize,
        session: u128,
        source: &crate::foreign_a11y::ForeignA11y,
    ) -> Self {
        let shared = Arc::new(Mutex::new(None));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let replay = Arc::new(AtomicBool::new(false));
        let adapter = super::a11y_bridge::install(
            event_loop,
            window,
            shared.clone(),
            actions.clone(),
            wake,
            replay.clone(),
        );
        let mut foreign = crate::foreign_a11y::ForeignA11y::new();
        foreign.sync_retained_from(source);
        Self {
            adapter,
            shared,
            actions,
            replay,
            foreign,
            routes: A11yRoutes::new(),
            scope: WindowScope {
                slot,
                instance: NEXT_WINDOW.fetch_add(1, Ordering::Relaxed),
                session,
            },
            frame: None,
            focus: None,
            placements: Vec::new(),
            frames: 30,
        }
    }
}
impl Shell {
    pub(super) fn sync_lens_semantics(&mut self) {
        for lens in self.lens_windows.values_mut() {
            lens.a11y.foreign.sync_retained_from(&self.surface_a11y);
        }
    }
    pub(super) fn retire_surface_semantics(&mut self, node: uuid::Uuid) {
        self.surface_a11y.retire(node);
        self.sync_lens_semantics();
    }
    pub(super) fn clear_surface_semantics(&mut self) {
        self.surface_a11y.clear();
        self.sync_lens_semantics();
    }
    pub(super) fn projected_lens_a11y(
        &self,
        ordinal: usize,
        w: u32,
        h: u32,
    ) -> Option<(uxtree::UxTree, A11yRoutes, Option<accesskit::NodeId>)> {
        let pane_rects = self
            .lens_plan(ordinal, w, h)
            .into_iter()
            .filter_map(|s| match s.kind {
                SurfaceKind::Pane(pane) => Some((pane, s.rect)),
                _ => None,
            })
            .collect();
        let focused_pane = match self.app.focus {
            FocusTarget::Pane(p) => Some(p),
            _ => None,
        };
        let contributed = crate::contributed_a11y::project(
            &self.renderers.contributed,
            &pane_rects,
            focused_pane,
        );
        let mut trees = contributed.trees;
        for (&pane, &rect) in &pane_rects {
            if matches!(self.app.pane_content(pane),Some(crate::panes::PaneContent::Registered(kind)) if kind.as_str()==crate::panes::kind::ARRANGE)
                && let Some(tree) = crate::arrange_pane::physics_refusal_tree(&self.app, pane, rect)
            {
                trees.insert(pane, tree);
            }
            if let Some(tree) = self
                .renderers
                .gloss
                .get(&pane)
                .and_then(|surface| surface.diagnostic_tree(pane, rect))
            {
                trees.insert(pane, tree);
            }
        }
        let (mut tree, mut routes) =
            crate::a11y::project_lens_with_contributions(&self.app, ordinal, trees)?;
        routes.extend(contributed.routes.into_iter().map(|(id, r)| {
            (
                id,
                A11yRoute::Contributed {
                    pane: r.pane,
                    generation: r.generation,
                    node: r.node,
                },
            )
        }));
        let mut focus = contributed.focus;
        let layout = self.app.lenses.get(ordinal)?.as_ref()?;
        let focused = self
            .outer_focus
            .as_ref()
            .is_some_and(|f| f.slot == ordinal + 1);
        if self
            .app
            .blueprint_space(crate::action::SpaceRef::Lens(ordinal))
            .is_none()
            && let Some(frame) = self
                .outer_frames
                .borrow()
                .get(&(ordinal + 1))
                .and_then(|f| f.accessibility(focused))
        {
            let parent = uxtree::node_id_for_path(&format!("frisket/{}", layout.id.as_str()));
            let bodies = crate::panes::pane_projection_ids(layout);
            if let Some((frame_routes, frame_focus)) =
                crate::frame_a11y::graft(&mut tree, parent, &bodies, frame)
            {
                routes.extend(
                    frame_routes
                        .into_iter()
                        .map(|(id, r)| (id, A11yRoute::Frame(r))),
                );
                if focused {
                    focus = frame_focus;
                }
            }
        }
        focus = focus.filter(|id| tree.nodes.iter().any(|(n, _)| n == id));
        Some((tree, routes, focus))
    }
    pub(super) fn push_lens_a11y_trees(&mut self) {
        let windows = self
            .lens_windows
            .iter()
            .map(|(id, l)| (*id, l.ordinal, l.width, l.height))
            .collect::<Vec<_>>();
        for (id, ordinal, w, h) in windows {
            self.push_lens_a11y_tree(id, ordinal, w, h, None);
        }
    }
    pub(super) fn withdraw_lens_a11y_departures(&mut self) {
        let windows = self
            .lens_windows
            .iter()
            .map(|(id, l)| (*id, l.ordinal, l.width, l.height, l.a11y.placements.clone()))
            .collect::<Vec<_>>();
        for (id, ordinal, w, h, previous) in windows {
            let current = self.window_foreign_a11y_placements(ordinal + 1);
            if previous
                .iter()
                .any(|(node, _)| !current.iter().any(|(n, _)| n == node))
            {
                let retained = current
                    .into_iter()
                    .filter(|(node, _)| previous.iter().any(|(n, _)| n == node))
                    .collect();
                self.push_lens_a11y_tree(id, ordinal, w, h, Some(retained));
            }
        }
    }
    fn push_lens_a11y_tree(
        &mut self,
        id: WindowId,
        ordinal: usize,
        w: u32,
        h: u32,
        override_placements: Option<Vec<(uuid::Uuid, accesskit::Rect)>>,
    ) {
        let slot = ordinal + 1;
        let forced = override_placements.is_some();
        let placements =
            override_placements.unwrap_or_else(|| self.window_foreign_a11y_placements(slot));
        let session = self.app.session_id.as_uuid().as_u128();
        // Projection prepares the same retained frame used for paint and hits.
        let Some((mut tree, mut routes, mut focus)) = self.projected_lens_a11y(ordinal, w, h)
        else {
            return;
        };
        let stamp = self.outer_frames.borrow().get(&slot).map(|f| f.stamp());
        let body_rects = self
            .lens_plan(ordinal, w, h)
            .into_iter()
            .filter_map(|s| match s.kind {
                SurfaceKind::Pane(p) => Some((p, s.rect)),
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>();
        let lens = self.lens_windows.get_mut(&id).unwrap();
        let a = &mut lens.a11y;
        if a.scope.session != session {
            a.scope.session = session;
            a.routes.clear();
            a.foreign.reset_publication();
            a.frames = 30;
        }
        let replay = a.replay.load(Ordering::Acquire);
        if replay {
            a.foreign.reset_publication();
        }
        a.frames = a.frames.saturating_add(1);
        if !forced
            && a.frames < 30
            && !replay
            && !a.foreign.needs_publish()
            && a.frame == stamp
            && a.focus == Some(self.app.focus)
            && a.placements == placements
        {
            return;
        }
        let mut visible = Vec::new();
        for &(node, rect) in &placements {
            let Some(mut graft) = a.foreign.graft(node, rect) else {
                continue;
            };
            graft.set_label("browser page");
            let doc = uxtree::node_id_for_path(&format!("turnstone/doc/{node}"));
            let layout = self.app.lenses[ordinal].as_ref().unwrap();
            let bodies = crate::panes::pane_projection_ids(layout);
            let parent = layout
                .iter_leaves()
                .find_map(|(pane, content, _)| {
                    let pane_id = bodies.get(&workbench::TileId(pane.0)).copied()?;
                    if !tree.nodes.iter().any(|(id, _)| *id == pane_id) {
                        return None;
                    }
                    let direct = matches!(content,crate::panes::PaneContent::Tile(n) if *n==node);
                    let workbench = matches!(content, crate::panes::PaneContent::Workbench)
                        && body_rects.get(&pane).is_some_and(|body| {
                            body.contains(
                                ((rect.x0 + rect.x1) / 2.0) as f32,
                                ((rect.y0 + rect.y1) / 2.0) as f32,
                            )
                        });
                    (direct || workbench).then_some(pane_id)
                })
                .unwrap_or(tree.root);
            if let Some((_, parent)) = tree.nodes.iter_mut().find(|(id, _)| *id == parent) {
                let mut children = parent.children().to_vec();
                children.push(doc);
                parent.set_children(children);
            }
            tree.nodes.push((doc, graft));
            visible.push(node);
            if self.outer_focus.as_ref().is_none_or(|f| f.slot != slot)
                && matches!(self.app.focus,FocusTarget::Content{node:focused,..} if focused==node)
            {
                focus = Some(doc);
            }
        }
        let map = crate::window_projection::namespace(&mut tree, &mut focus, a.scope);
        routes = routes
            .into_iter()
            .filter_map(|(id, r)| map.get(&id).map(|new| (*new, r)))
            .collect();
        let mut host = tree.to_tree_update(focus);
        let bounds = accesskit::Rect::new(0.0, 0.0, w.max(1) as f64, h.max(1) as f64);
        for (_, node) in &mut host.nodes {
            if node.bounds().is_none() {
                node.set_bounds(bounds);
            }
        }
        let prepared = a
            .foreign
            .initial_host(host.clone(), &visible)
            .and_then(|initial| {
                a.foreign
                    .frame(host, &visible)
                    .map(|updates| (initial, updates))
            });
        let (initial, updates) = match prepared {
            Ok(p) => p,
            Err(reason) => {
                tracing::warn!(%reason,slot,"lens accessibility publication refused");
                return;
            },
        };
        a.routes = routes;
        a.frame = stamp;
        a.focus = Some(self.app.focus);
        a.placements = placements;
        a.frames = 0;
        *a.shared.lock().expect("lens a11y tree poisoned") = Some(initial.clone());
        let mut aborted = false;
        for update in updates {
            a.adapter.update_if_active(|| {
                let (update, abort) =
                    crate::foreign_a11y::activation_safe_update(&a.replay, update, &initial);
                aborted = abort;
                update
            });
            if aborted {
                break;
            }
        }
        if aborted {
            a.foreign.reset_publication();
            lens.window.request_redraw();
        }
    }
    pub(super) fn drain_lens_a11y_actions(&mut self) {
        let batches = self
            .lens_windows
            .iter_mut()
            .map(|(id, l)| {
                (
                    *id,
                    l.ordinal,
                    l.width,
                    l.height,
                    std::mem::take(&mut *l.a11y.actions.lock().expect("lens a11y queue poisoned")),
                )
            })
            .collect::<Vec<_>>();
        for (window, ordinal, w, h, requests) in batches {
            if !requests.is_empty() {
                if let Some(lens) = self.lens_windows.get_mut(&window) {
                    lens.a11y.frames = 30;
                }
            }
            for request in requests {
                let slot = ordinal + 1;
                if request.target_tree != accesskit::TreeId::ROOT {
                    let result = self
                        .lens_windows
                        .get(&window)
                        .ok_or("Lens is closed".to_owned())
                        .and_then(|l| l.a11y.foreign.action_target(&request))
                        .and_then(|node| {
                            if !self
                                .window_foreign_a11y_placements(slot)
                                .iter()
                                .any(|(id, _)| *id == node)
                            {
                                return Err("Foreign surface moved to another window".into());
                            }
                            self.content_sessions
                                .surface_mut(&node)
                                .ok_or("Foreign producer retired".to_owned())?
                                .send_accessibility_action(request)
                                .map_err(|e| e.to_string())
                        });
                    if let Err(reason) = result {
                        self.a11y_window_miss(reason);
                    }
                    continue;
                }
                let route = self
                    .lens_windows
                    .get(&window)
                    .and_then(|l| l.a11y.routes.get(&request.target_node))
                    .cloned();
                match route {
                    Some(A11yRoute::Frame(route)) => {
                        if !self.apply_frame_a11y(slot, &route, request.action) {
                            self.a11y_window_miss(
                                "Lens frame control is stale, hidden or unsupported".into(),
                            );
                        }
                    },
                    Some(A11yRoute::Contributed {
                        pane,
                        generation,
                        node,
                    }) => {
                        let spec = self.contributed_pane_spec(pane, self.app.pane_content(pane));
                        let visible = self.app.space_of(pane)
                            == Some(crate::action::SpaceRef::Lens(ordinal))
                            && self
                                .lens_plan(ordinal, w, h)
                                .iter()
                                .any(|s| s.kind == SurfaceKind::Pane(pane));
                        let landed = visible
                            && crate::contributed_a11y::apply_route(
                                &mut self.renderers.contributed,
                                spec.as_ref(),
                                crate::contributed_a11y::ContributedA11yRoute {
                                    pane,
                                    generation,
                                    node,
                                },
                                request.action,
                            );
                        if !landed {
                            self.a11y_window_miss(
                                "Lens contributed control is stale or hidden".into(),
                            );
                        } else {
                            self.leave_frame_focus();
                            self.app.active_pane = Some(pane);
                            self.app.focus = FocusTarget::Pane(pane);
                            self.raise_shared_float(pane);
                        }
                    },
                    Some(route @ A11yRoute::SelectMember { pane, .. })
                        if request.action == accesskit::Action::Click
                            && self.app.space_of(pane)
                                == Some(crate::action::SpaceRef::Lens(ordinal)) =>
                    {
                        self.leave_frame_focus();
                        let effects = crate::a11y::apply_route(
                            &mut self.app,
                            Some(&route),
                            request.target_node,
                        );
                        self.run_effects(effects);
                    },
                    _ => self.a11y_window_miss(
                        "Lens action names an absent, stale or unsupported target".into(),
                    ),
                }
            }
        }
        self.request_redraw();
    }
    fn a11y_window_miss(&mut self, target: String) {
        self.app.note(crate::observe::AppEvent::InteractionMissed {
            what: "a11y-action",
            target,
        });
    }
}
