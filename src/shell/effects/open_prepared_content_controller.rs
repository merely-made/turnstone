// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Construct a real controller before accepting source or installing its entry.
pub(super) fn open_prepared_content_controller(
    app: &crate::app::App,
    prepared: PreparedContentController,
    engines: std::sync::Arc<inker::SessionRegistry<netrender::Scene>>,
    surfaces: std::sync::Arc<inker::SurfaceEngineRegistry>,
    clock: impl pelt_core::PeltClock,
) -> Result<Option<OpenedContentController>, String> {
    let local_micron = prepared.config.engine_id == crate::nomadnet::ENGINE_ID
        && prepared.url.starts_with("file://");
    if local_micron && !content_spawn_is_current(app, prepared.node, &prepared.url) {
        return Ok(None);
    }
    let controller =
        pelt_core::PeltController::new_shared(engines, surfaces, prepared.config, clock)?;
    if local_micron && !content_spawn_is_current(app, prepared.node, &prepared.url) {
        return Ok(None);
    }
    let source = prepared
        .source
        .map(|document| Update::ContentSourceCaptured {
            node: prepared.node,
            url: prepared.url,
            document,
        });
    Ok(Some(OpenedContentController { controller, source }))
}
