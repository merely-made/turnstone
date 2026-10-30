// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl NodeSessions for NodeDocuments {
    fn session(&self, node: &uuid::Uuid) -> Option<&dyn DocumentSession<Scene>> {
        self.get(node)
            .and_then(pelt_core::PeltContent::document)
            .map(pelt_core::PeltController::session)
    }

    fn session_mut(&mut self, node: &uuid::Uuid) -> Option<&mut dyn DocumentSession<Scene>> {
        self.get_mut(node)
            .and_then(pelt_core::PeltContent::document_mut)
            .map(pelt_core::PeltController::session_mut)
    }

    fn sessions(&self) -> impl Iterator<Item = &dyn DocumentSession<Scene>> {
        self.values()
            .filter_map(pelt_core::PeltContent::document)
            .map(pelt_core::PeltController::session)
    }
}
