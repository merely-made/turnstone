// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotVisitRegistry {
    pub(crate) fn set_local_root(&mut self, root: Option<[u8; 32]>) {
        self.local_root = root;
    }

    pub(crate) fn is_local(&self, root: &[u8; 32]) -> bool {
        self.local_root.as_ref() == Some(root)
    }

    pub(crate) fn holder(&self, root: &[u8; 32]) -> Option<Arc<KnotHub>> {
        self.holders.get(root).cloned()
    }

    pub(crate) fn insert_holder(&mut self, root: [u8; 32], hub: Arc<KnotHub>) {
        self.holders.insert(root, hub);
    }

    /// Let go of every visit. Called when the place closes: a visiting thread
    /// drives its carrier on the lane runtime, so it must stop before that
    /// runtime does.
    pub(crate) fn clear(&mut self) {
        self.holders.clear();
        self.local_root = None;
    }
}
