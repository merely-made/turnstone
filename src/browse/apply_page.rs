// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Fold one completed page fetch into the graph: stamp the response's
/// Content-Type as the requesting node's MIME hint, and for HTML extract the
/// page `<title>` (render-free static parse) so the canvas caption flips from
/// the host fallback to the real title, then chase the page's favicon so the
/// node face wears a real icon. All stamps target the requester by member id.
pub fn apply_page(
    canvas: &mut Canvas,
    node: Uuid,
    url: String,
    result: Result<FetchedPage, String>,
) -> Vec<Effect> {
    if !still_current(canvas, node, &url) {
        tracing::info!(%node, %url, "page result for a superseded node; dropped");
        return Vec::new();
    }
    let mut effects = Vec::new();
    match result {
        Ok(fetched) => {
            let media = fetched
                .content_type
                .as_deref()
                .and_then(|ct| ct.split(';').next())
                .map(|m| m.trim().to_ascii_lowercase());
            tracing::info!(%url, content_type = ?media, bytes = fetched.body.len(), "page fetched");
            canvas.set_node_mime_hint_for(node, media.clone());
            if media.as_deref() == Some("text/html") {
                let doc = genet_static_dom::StaticDocument::parse(&fetched.body);
                if let Some(title) = fleece::extract(&doc).title {
                    if canvas.set_node_title_for(node, title.clone()) {
                        tracing::info!(%url, %title, "node title enriched from the page");
                    }
                }
                // L1 retains the address/title history, not extracted page
                // content. Source bytes are admitted only through explicit
                // source capture; keep this fetch path metadata-only.
                // Best-effort: chase the page's favicon; the bytes route back
                // as a FaviconFetched update correlated to this node.
                if let Some(icon_url) = favicon_url_for(&url, &doc) {
                    effects.push(Effect::FetchFavicon {
                        node,
                        owner_url: url.clone(),
                        url: icon_url,
                    });
                }
            }
        },
        Err(err) => {
            tracing::warn!(%url, %err, "page fetch failed");
        },
    }
    effects.push(Effect::SaveSession);
    effects.push(Effect::Redraw);
    effects
}
