// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Knot publishing, which signs as the profile root.
#[cfg(feature = "peers")]
pub(super) fn start_publishing(
    source: crate::publish_service::PublishSource,
    root: Arc<crate::identity::RootIdentity>,
) -> Option<Arc<crate::publish_service::KnotPublishingService>> {
    match crate::publish_service::KnotPublishingService::start(source, root) {
        Ok(service) => Some(Arc::new(service)),
        Err(error) => {
            tracing::warn!(%error, "Knot publishing is unavailable");
            None
        },
    }
}

#[cfg(not(feature = "peers"))]
pub(super) fn start_publishing(
    _: crate::publish_service::PublishSource,
    _: Arc<crate::identity::RootIdentity>,
) -> Option<Arc<crate::publish_service::KnotPublishingService>> {
    None
}
