// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) struct PrivateBinding {
    pub(super) root: Arc<crate::identity::RootIdentity>,
    pub(super) bound: Arc<crate::custody_binding::BoundSealer>,
    pub(super) protection: Arc<dyn athanor::workshop_seal::Protection>,
    pub(super) watcher: crate::custody_binding::Watcher,
    pub(super) events: Receiver<crate::custody_binding::Event>,
}
