// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The share reader, which speaks as the profile root.
#[cfg(feature = "peers")]
pub(super) fn start_share_reader(
    root: Arc<crate::identity::RootIdentity>,
) -> Option<Arc<crate::share_reader_service::KnotShareReaderService>> {
    match crate::share_reader_service::KnotShareReaderService::start(root) {
        Ok(service) => Some(Arc::new(service)),
        Err(error) => {
            tracing::warn!(%error, "Knot share reader is unavailable");
            None
        },
    }
}

#[cfg(not(feature = "peers"))]
pub(super) fn start_share_reader(
    _: Arc<crate::identity::RootIdentity>,
) -> Option<Arc<crate::share_reader_service::KnotShareReaderService>> {
    None
}
