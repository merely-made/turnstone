// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Prepare through the same host boundary used by ordinary SpawnContent.
pub(super) fn prepare_content_controller(
    app: &crate::app::App,
    node: uuid::Uuid,
    url: &str,
    engine_id: &str,
    fetched: Option<crate::content::FetchedDocument>,
    clip: Option<inker::EngineDocument>,
    viewport: (u32, u32),
) -> Result<Option<PreparedContentController>, String> {
    if engine_id == crate::nomadnet::ENGINE_ID
        && url.starts_with("file://")
        && !content_spawn_is_current(app, node, url)
    {
        return Ok(None);
    }
    let (fetched, source) = if engine_id == crate::nomadnet::ENGINE_ID
        && url.starts_with("file://")
        && fetched.is_none()
    {
        let bytes =
            mere_document_lanes::ResourceFetcher::fetch(&genet_documents::LocalFetcher, url)
                .ok_or_else(|| format!("could not load {url} ({engine_id})"))?;
        let document = crate::content::FetchedDocument {
            body: String::from_utf8_lossy(&bytes).into_owned(),
            bytes,
            content_type: None,
            effective_url: Some(url.to_owned()),
            acquired_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0),
        };
        (Some(document.clone()), Some(document))
    } else {
        (fetched, None)
    };
    let mut spawn = SessionSpawnRequest::new(url).with_viewport(viewport.0, viewport.1);
    if let Some(document) = clip {
        let body = serde_json::to_string(&document).map_err(|error| error.to_string())?;
        spawn = spawn
            .with_body(body)
            .with_content_type(document.content_type);
    } else if let Some(document) = fetched {
        spawn = spawn.with_body(document.body);
        if let Some(content_type) = document.content_type {
            spawn = spawn.with_content_type(content_type);
        }
    }
    let mut config =
        pelt_core::PeltControllerConfig::from_request(engine_id, spawn).with_host_history();
    if config.request.body.is_some() {
        config = config.with_host_loading();
    }
    Ok(Some(PreparedContentController {
        node,
        url: url.to_owned(),
        config,
        source,
    }))
}
