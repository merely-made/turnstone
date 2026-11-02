// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for KnotDocumentSession {
    fn drop(&mut self) {
        let _ = self.hub.commands.send(HubCommand::Unregister {
            registration: self.registration,
        });
    }
}
