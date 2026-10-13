// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Window host namespaces and publication use the one Genet native adapter.
use super::Shell;
use crate::{a11y::{A11yRoute, A11yRoutes}, surface::{FocusTarget, SurfaceKind}, window_projection::WindowScope};
use std::sync::atomic::{AtomicU64, Ordering};
use winit::window::WindowId;
static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);
pub(super) fn scope(slot: usize, session: u128) -> WindowScope {
    WindowScope { slot, instance: NEXT_WINDOW.fetch_add(1, Ordering::Relaxed), session }
}
impl Shell {
    pub(super) fn sync_lens_semantics(&mut self) {
        for lens in self.lens_windows.values_mut() { lens.a11y_forest.sync_retained_from(&self.surface_a11y); }
    }
    pub(super) fn retire_surface_semantics(&mut self, node: uuid::Uuid) { self.surface_a11y.retire(node); self.sync_lens_semantics(); }
    pub(super) fn clear_surface_semantics(&mut self) { self.surface_a11y.clear(); self.sync_lens_semantics(); }
    pub(super) fn projected_lens_a11y(
        &mut self,
        ordinal: usize,
        w: u32,
        h: u32,
    ) -> Option<super::document_a11y::Projected> {
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
        let guests = contributed.guests;
        let admissions = contributed.routes.into_iter().map(|(id, route)| (id, super::a11y_bridge::Admission::Contributed(route))).collect();
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
        let mut focus = contributed.focus;
        let layout = self.app.lenses.get(ordinal)?.as_ref()?;
        let focused = super::frame_keys::header_focus_in_window(self.outer_focus.as_ref(), ordinal as u64 + 1, self.app.session_id.as_uuid());
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
        let mut frame = super::document_a11y::Projected { tree, routes, focus, guests, admissions };
        let plan = self.lens_plan(ordinal, w, h);
        let header = focused.then_some(frame.focus).flatten();
        self.project_document_accessibility(&mut frame, &plan);
        frame.focus = super::frame_keys::accessibility_focus(None, header, frame.focus);
        let mounted: std::collections::HashSet<_> = frame.tree.nodes.iter().filter_map(|(_, n)| n.tree_id()).collect();
        frame.guests.retain(|g| mounted.contains(&g.tree_id));
        frame.admissions.retain(|(tree, _), _| mounted.contains(tree));
        Some(frame)
    }
    pub(super) fn push_lens_a11y_trees(&mut self) {
        let mut windows: Vec<_> = self.lens_windows.iter().map(|(id,l)| (*id,l.ordinal,l.width,l.height)).collect();
        windows.sort_by_key(|(_, ordinal, _, _)| *ordinal);
        for (id,ordinal,w,h) in windows { self.push_lens_a11y_tree(id,ordinal,w,h,None); }
    }
    pub(super) fn withdraw_lens_a11y_departures(&mut self) {
        let windows: Vec<_> = self.lens_windows.iter().map(|(id,l)| (*id,l.ordinal,l.width,l.height,l.a11y_placements.clone())).collect();
        for (id,ordinal,w,h,previous) in windows {
            let current = self.window_foreign_a11y_placements(ordinal+1);
            if previous.iter().any(|(node,_)| !current.iter().any(|(n,_)| n==node)) {
                let retained = current.into_iter().filter(|(node,_)| previous.iter().any(|(n,_)| n==node)).collect();
                self.push_lens_a11y_tree(id,ordinal,w,h,Some(retained));
            }
        }
    }
    fn push_lens_a11y_tree(&mut self, id: WindowId, ordinal: usize, w: u32, h: u32,
        override_placements: Option<Vec<(uuid::Uuid,accesskit::Rect)>>) {
        let placements = override_placements.unwrap_or_else(|| self.window_foreign_a11y_placements(ordinal+1));
        let Some(mut frame) = self.projected_lens_a11y(ordinal,w,h) else { return; };
        let session = self.app.session_id.as_uuid().as_u128();
        let Some(lens) = self.lens_windows.get_mut(&id) else { return; };
        lens.a11y_forest.sync_retained_from(&self.surface_a11y);
        let epoch = lens.a11y.activation_epoch();
        if lens.a11y_epoch != Some(epoch) || lens.a11y_scope.session != session {
            lens.a11y_forest.reset_publication();
            lens.a11y_actions.lock().expect("lens a11y queue poisoned").publish(Default::default());
            lens.a11y_epoch = Some(epoch);
            lens.a11y_scope.session = session;
        }
        let header_focused = super::frame_keys::header_focus_in_window(self.outer_focus.as_ref(), ordinal as u64 + 1, self.app.session_id.as_uuid());
        let projected_focus = frame.focus;
        let visible = super::lens::append_foreign_lens(&mut frame,&lens.a11y_forest,placements.clone(),self.app.focus);
        if header_focused { frame.focus = projected_focus; }
        let map = crate::window_projection::namespace(&mut frame.tree,&mut frame.focus,lens.a11y_scope);
        frame.routes = frame.routes.into_iter().filter_map(|(id,route)| map.get(&id).map(|new| (*new,route))).collect();
        frame.admissions = frame.admissions.into_iter().filter_map(|((tree,id),admission)| {
            if tree == accesskit::TreeId::ROOT { map.get(&id).map(|new| ((tree,*new),admission)) }
            else { Some(((tree,id),admission)) }
        }).collect();
        let mut update = frame.tree.to_tree_update(frame.focus);
        let bounds = accesskit::Rect::new(0.0,0.0,w.max(1) as f64,h.max(1) as f64);
        for (_,node) in &mut update.nodes { if node.bounds().is_none() { node.set_bounds(bounds); } }
        lens.a11y_tree = Some(update.clone());
        let initial = super::a11y_bridge::safe_initial(&update);
        let updates = match lens.a11y_forest.frame_with_guests(update,&visible,&frame.guests) {
            Ok(updates) => updates,
            Err(error) => { tracing::warn!(%error,"lens accessibility composition refused"); return; }
        };
        let served = lens.a11y.update_batch_at(epoch,initial,updates).unwrap_or_else(|error| {
            tracing::warn!(%error,"lens accessibility publication refused"); false
        });
        lens.a11y_actions.lock().expect("lens a11y queue poisoned").publish(if served { frame.admissions } else { Default::default() });
        lens.a11y_routes = frame.routes;
        lens.a11y_placements = placements;
        if !served { lens.a11y_forest.retry_publication(); }
    }
    // Scenario injection and platform wakeups share the same epoch-aware drain.
    pub(super) fn drain_lens_a11y_actions(&mut self) { self.drain_a11y_actions(); }
}
