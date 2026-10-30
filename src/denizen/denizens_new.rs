// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Denizens {
    /// A runtime whose delegation chains must root at `root` — the profile
    /// identity's master key. A table rooted anywhere else verifies nothing,
    /// so this is set at construction, never after certificates arrive.
    pub fn new(root: [u8; 32]) -> Self {
        Self {
            authority: DelegationTable::new(root),
            ..Self::default()
        }
    }

    /// A runtime with no profile root yet: the vault is locked, so it holds
    /// no certificate and covers nothing (vault lock ruling 35). The adopt
    /// path rebuilds it under the root once the vault unlocks.
    pub fn pending() -> Self {
        Self::default()
    }

    /// Whether any participant resides in the session.
    pub fn is_empty(&self) -> bool {
        self.residents.is_empty()
    }
}
