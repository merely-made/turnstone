// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Native producer semantics, driven on the existing UI thread.

use super::{NodeSurfaces, Shell};
use crate::surface::{FocusTarget, SurfaceKind};

impl Shell {
    pub(super) fn activate_surface_accessibility(&mut self, node: uuid::Uuid, resync: bool) {
        let Some(producer) = self.content_sessions.surface_mut(&node) else {
            return;
        };
        let root = if resync {
            producer.request_accessibility_resync().map(Some)
        } else {
            producer.set_accessibility_active(true)
        };
        match root {
            Ok(Some(root)) => {
                if let Err(error) = self.surface_a11y.admit(node, root) {
                    tracing::warn!(%node, %error, "foreign accessibility admission refused");
                }
                self.sync_lens_semantics();
            },
            Ok(None) | Err(inker::SurfaceError::Unsupported(_)) => {},
            Err(error) => tracing::warn!(%node, %error, "foreign accessibility activation refused"),
        }
    }

    pub(super) fn resync_surface_accessibility(&mut self, node: uuid::Uuid) {
        // Retire and publish before upstream reactivation can return a new ID.
        // Old queued assistive requests then cannot target the replaced page.
        // Navigation also recovers a previously withdrawn or incomplete tree.
        self.retire_surface_semantics(node);
        self.push_a11y_tree();
        self.activate_surface_accessibility(node, true);
    }

    pub(super) fn drain_surface_accessibility(&mut self) {
        const MAX_UPDATES: usize = 256;
        let mut changed = false;
        for (node, producer) in self.content_sessions.surfaces_mut() {
            for _ in 0..MAX_UPDATES {
                let Some(update) = producer.poll_accessibility_update() else {
                    break;
                };
                changed = true;
                if let Err(error) = self.surface_a11y.apply(node, update) {
                    // Stale callbacks do not initiate a reactivation loop. A
                    // malformed retained forest withdraws its own publication.
                    tracing::debug!(%node, %error, "foreign accessibility update refused");
                }
            }
        }
        if changed {
            self.sync_lens_semantics();
        }
    }

    pub(super) fn foreign_a11y_placements(&self) -> Vec<(uuid::Uuid, accesskit::Rect)> {
        self.window_foreign_a11y_placements(0)
    }
    pub(super) fn window_foreign_a11y_placements(
        &self,
        slot: usize,
    ) -> Vec<(uuid::Uuid, accesskit::Rect)> {
        let mut surfaces = vec![(0, self.surface_plan())];
        let mut lenses = self.lens_windows.values().collect::<Vec<_>>();
        lenses.sort_by_key(|l| l.ordinal);
        surfaces.extend(
            lenses
                .into_iter()
                .map(|l| (l.ordinal + 1, self.lens_plan(l.ordinal, l.width, l.height))),
        );
        let appearances = surfaces.into_iter().flat_map(|(slot, plan)| {
            plan.into_iter().filter_map(move |surface| {
                let SurfaceKind::Content(node) = surface.kind else {
                    return None;
                };
                self.surface_a11y
                    .ready(node)
                    .then_some(crate::window_projection::Appearance {
                        node,
                        slot,
                        appearance: surface.id.0,
                        rect: accesskit::Rect::new(
                            surface.rect.x as f64,
                            surface.rect.y as f64,
                            (surface.rect.x + surface.rect.w) as f64,
                            (surface.rect.y + surface.rect.h) as f64,
                        ),
                    })
            })
        });
        let focused = match self.app.focus {
            FocusTarget::Content { node, appearance } => Some((node, appearance.0)),
            _ => None,
        };
        crate::window_projection::owners(appearances, focused)
            .into_values()
            .filter(|a| a.slot == slot)
            .map(|a| (a.node, a.rect))
            .collect()
    }
}
