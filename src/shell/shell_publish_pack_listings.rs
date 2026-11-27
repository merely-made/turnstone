// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Hand the settings pane the command pack and mod project lists.
    pub(super) fn publish_pack_listings(&mut self) {
        let packs = self.app.packs.as_ref().map(|packs| packs.listing());
        let mods = self.app.mods.as_ref().map(|mods| mods.listing());
        let host = self.app.mods.clone();
        let kinds_changed = match (&host, &mods) {
            (Some(host), Some(listing)) => self.sync_viewer_kinds(host, listing),
            _ => false,
        };
        let packs_changed = packs.is_some_and(|l| self.live_settings.publish_command_packs(l));
        let mods_changed = mods.is_some_and(|l| self.live_settings.publish_mod_projects(l));
        if packs_changed || mods_changed || kinds_changed {
            self.request_redraw();
        }
    }

    /// Make the viewer pane kinds follow the project list, so an install, uninstall, reload
    /// that changes the kind, or a retitle reaches the palette and the panes on the next frame
    /// (mod projects MP7c, D63).
    fn sync_viewer_kinds(
        &mut self,
        host: &std::sync::Arc<crate::mod_projects::ModProjectHost>,
        listing: &crate::mod_projects::ProjectListing,
    ) -> bool {
        let viewers: Vec<(String, String)> = listing
            .rows
            .iter()
            .filter(|row| row.kind == "viewer")
            .map(|row| (row.id.clone(), row.title.clone()))
            .collect();
        let synced = crate::mod_viewer::sync_kinds(
            &mut self.surface_providers,
            &viewers,
            host,
            &self.mod_viewer,
        );
        // A pane of a kind that came or went starts from nothing: a placeholder it had must not
        // keep the pane, and an open session must not outlive its provider.
        for kind in synced.removed.iter().chain(&synced.added) {
            // One renderer table serves every window, so a torn-out pane is evicted the same way.
            for pane in self.app.panes_of_kind(kind) {
                self.renderers.evict(pane);
            }
            self.renderers.contributed.drop_kind(kind);
        }
        synced.changed
    }
}
