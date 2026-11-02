// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn dispatch_content_spawn(
    engine_id: &str,
    host_ready: bool,
    pending: &mut Vec<(uuid::Uuid, String)>,
    node: uuid::Uuid,
    url: &str,
) -> ContentSpawnDispatch {
    if !matches!(
        engine_id,
        inker::routing::ENGINE_WELD_CHROMIUM
            | inker::routing::ENGINE_SCRYING_WEB
            | inker::routing::ENGINE_GRAFT_SERVO
    ) {
        return ContentSpawnDispatch::Session;
    }
    if !host_ready {
        pending.push((node, url.to_string()));
        return ContentSpawnDispatch::PendingBrowser;
    }
    ContentSpawnDispatch::Browser
}
