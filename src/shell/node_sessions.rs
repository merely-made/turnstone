// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The session seam of each node's document, for the host features the Pelt
/// controller does not wrap yet (find, subresources, Turnstone's own pointer
/// path, engine extras reached by downcast).
pub(crate) trait NodeSessions {
    fn session(&self, node: &uuid::Uuid) -> Option<&dyn DocumentSession<Scene>>;
    fn session_mut(&mut self, node: &uuid::Uuid) -> Option<&mut dyn DocumentSession<Scene>>;
    fn sessions(&self) -> impl Iterator<Item = &dyn DocumentSession<Scene>>;
}
