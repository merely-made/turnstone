// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use fetch::peer::PeerAssets;
use std::sync::Arc;

/// Exact source and its verified same-root resources, after a profile-authorized read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Replay {
    pub body: String,
    pub assets: Arc<PeerAssets>,
}

pub(crate) fn engines(assets: Arc<PeerAssets>) -> Arc<inker::SessionRegistry<netrender::Scene>> {
    let held = pelt_core::native::HeldResources(assets.resources.clone());
    Arc::new(pelt_core::native::sessions_with_resources(held))
}
