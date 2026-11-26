// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Shell {
    /// Hand the settings pane the command pack and mod project lists.
    pub(super) fn publish_pack_listings(&mut self) {
        let packs = self.app.packs.as_ref().map(|packs| packs.listing());
        let mods = self.app.mods.as_ref().map(|mods| mods.listing());
        let packs_changed = packs.is_some_and(|l| self.live_settings.publish_command_packs(l));
        let mods_changed = mods.is_some_and(|l| self.live_settings.publish_mod_projects(l));
        if packs_changed || mods_changed {
            self.request_redraw();
        }
    }
}
