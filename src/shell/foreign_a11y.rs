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
            },
            Ok(None) | Err(inker::SurfaceError::Unsupported(_)) => {},
            Err(error) => tracing::warn!(%node, %error, "foreign accessibility activation refused"),
        }
    }

    pub(super) fn resync_surface_accessibility(&mut self, node: uuid::Uuid) {
        // Retire and publish before upstream reactivation can return a new ID.
        // Old queued assistive requests then cannot target the replaced page.
        // Navigation also recovers a previously withdrawn or incomplete tree.
        self.surface_a11y.retire(node);
        self.push_a11y_tree();
        self.activate_surface_accessibility(node, true);
    }

    pub(super) fn drain_surface_accessibility(&mut self) {
        const MAX_UPDATES: usize = 256;
        for (node, producer) in self.content_sessions.surfaces_mut() {
            for _ in 0..MAX_UPDATES {
                let Some(update) = producer.poll_accessibility_update() else {
                    break;
                };
                if let Err(error) = self.surface_a11y.apply(node, update) {
                    // Stale callbacks do not initiate a reactivation loop. A
                    // malformed retained forest withdraws its own publication.
                    tracing::debug!(%node, %error, "foreign accessibility update refused");
                }
            }
        }
    }

    pub(super) fn foreign_a11y_placements(&self) -> Vec<(uuid::Uuid, accesskit::Rect)> {
        let mut placements = std::collections::BTreeMap::new();
        for surface in self.surface_plan() {
            let SurfaceKind::Content(node) = surface.kind else {
                continue;
            };
            if !self.surface_a11y.ready(node) {
                continue;
            }
            let rect = accesskit::Rect::new(
                surface.rect.x as f64,
                surface.rect.y as f64,
                (surface.rect.x + surface.rect.w) as f64,
                (surface.rect.y + surface.rect.h) as f64,
            );
            let focused = matches!(self.app.focus, FocusTarget::Content { node: current, appearance }
                if current == node && appearance == surface.id);
            // One native producer has one tree. A repeated visual appearance
            // does not create a second parent for that same tree.
            if focused || !placements.contains_key(&node) {
                placements.insert(node, rect);
            }
        }
        placements.into_iter().collect()
    }
}
