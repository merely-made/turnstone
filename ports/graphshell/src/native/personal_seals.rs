// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Borrowed protection for private sibling lanes on a personal graph.
//!
//! The personal-sync host alone changes membership and installs epochs. A
//! sibling can seal and open bytes, but cannot export keys or create a group.

use std::{
    collections::BTreeSet,
    sync::{Arc, RwLock},
};

use stickleback::{DataKeyring, GroupCiphertext};

pub(crate) struct SealingState {
    pub keys: Option<Arc<DataKeyring>>,
    pub current_roots: BTreeSet<[u8; 32]>,
    pub historical_roots: BTreeSet<[u8; 32]>,
    pub can_write: bool,
    pub revision: u64,
}

/// Live graph protection. Retained epochs have exactly the personal graph's
/// history semantics; retiring a reader cannot revoke bytes it already read.
#[derive(Clone)]
pub struct PersonalGraphSeals(pub(crate) Arc<RwLock<SealingState>>);

impl PersonalGraphSeals {
    /// Changes with installed epochs and group membership.
    pub fn revision(&self) -> u64 {
        self.0.read().unwrap().revision
    }

    /// Whether this device holds a current seat and a data epoch.
    pub fn can_write(&self) -> bool {
        self.0.read().unwrap().can_write
    }

    /// Current epochs require a current key-group seat. Old epochs remain
    /// admissible for attested roots known through the retained key agreement,
    /// so a newly keyed device can receive history written by a retired reader.
    /// Transport reachability alone confers no authority.
    pub fn admits(&self, root: &[u8; 32], sealed: &GroupCiphertext) -> bool {
        let state = self.0.read().unwrap();
        state.current_roots.contains(root)
            || (state.historical_roots.contains(root)
                && state.keys.as_ref().is_some_and(|keys| {
                    keys.contains(&sealed.epoch) && keys.current_epoch() != Some(sealed.epoch)
                }))
    }

    /// Seal under the current graph epoch, refusing an unkeyed or removed seat.
    pub fn seal(&self, clear: &[u8]) -> Result<GroupCiphertext, String> {
        let state = self.0.read().unwrap();
        if !state.can_write {
            return Err("this device is not a keyed reader of the personal graph".into());
        }
        state
            .keys
            .as_ref()
            .ok_or("personal graph has no data key")?
            .seal_random(clear)
            .map_err(|error| error.to_string())
    }

    /// Read a retained graph epoch, including history from before revocation.
    pub fn open(&self, sealed: &GroupCiphertext) -> Result<Vec<u8>, String> {
        self.0
            .read()
            .unwrap()
            .keys
            .as_ref()
            .ok_or("this device has no personal graph data key")?
            .open(sealed)
            .map_err(|error| error.to_string())
    }
}
