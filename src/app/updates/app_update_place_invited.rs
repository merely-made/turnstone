// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(super) fn update_place_invited(&mut self, update: Update) -> Vec<Effect> {
        match update {
            Update::PlaceInvited {
                session,
                generation,
                result,
            } => {
                let Some(pending) = self.pending_place_artifact.take_if(|pending| {
                    session == self.session_id && pending.generation == generation
                }) else {
                    return Vec::new();
                };
                match result {
                    Ok(invite) => match serde_json::to_vec_pretty(&*invite) {
                        Ok(bytes) => vec![Effect::WritePlaceArtifact {
                            path: pending.path,
                            bytes,
                        }],
                        Err(error) => self.refuse_place(format!("encode invitation: {error}")),
                    },
                    Err(error) => self.refuse_place(format!("authoring an invitation: {error}")),
                }
            },
            Update::PlaceJoined {
                session,
                generation,
                result,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    return Vec::new();
                }
                self.place = match result {
                    Ok((binding, snapshot)) => {
                        self.reconcile_shared_graph(&snapshot.shared);
                        crate::place::PlaceState::Offline {
                            binding,
                            generation,
                            snapshot,
                        }
                    },
                    // A refused invitation is not a degraded place, it is no
                    // place. Landing in `Degraded` would leave app state
                    // holding a binding admission never granted.
                    Err(error) => {
                        tracing::warn!(%error, "invitation refused");
                        self.events.push(AppEvent::PlaceRefused(error.clone()));
                        crate::place::PlaceState::Failed { error }
                    },
                };
                self.reflow_omnibar();
                vec![Effect::Redraw]
            },
            Update::PlaceOpened {
                session,
                generation,
                result,
            } => {
                if session != self.session_id || self.place.generation() != Some(generation) {
                    return Vec::new();
                }
                let Some(binding) = self.place.binding().cloned() else {
                    return Vec::new();
                };
                self.place = match result {
                    Ok(snapshot) => {
                        self.reconcile_shared_graph(&snapshot.shared);
                        crate::place::PlaceState::Offline {
                            binding,
                            generation,
                            snapshot,
                        }
                    },
                    Err(error) => {
                        tracing::warn!(%error, "place cache failed to open");
                        crate::place::PlaceState::Degraded {
                            binding,
                            generation,
                            error,
                        }
                    },
                };
                self.reflow_omnibar();
                vec![Effect::Redraw]
            },
            _ => unreachable!("dispatch group disagrees with exhaustive routing"),
        }
    }

    pub(super) fn begin_download(
        &mut self,
        node: uuid::Uuid,
        url: String,
        fetched: crate::action::FetchedPage,
    ) -> Vec<Effect> {
        self.content.note_closed(node);
        let received_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let byte_size = fetched.bytes.len() as u64;
        let media_type = fetched
            .content_type
            .as_deref()
            .and_then(|value| value.split(';').next())
            .map(|value| value.trim().to_ascii_lowercase());
        let title =
            crate::download::suggested_filename(&url, fetched.content_disposition.as_deref());
        let Some(graph) = self.graph_runtimes.graph_containing_member(node) else {
            return Vec::new();
        };
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return Vec::new();
        };
        canvas.set_node_mime_hint_for(node, media_type.clone());
        canvas.set_node_title_for(node, title);
        if let Err(error) = crate::content_classes::set_download_record(
            canvas,
            node,
            crate::content_classes::DownloadFacetRecord {
                source_url: &url,
                received_at_ms,
                byte_size,
                status: "storing",
                media_type: media_type.as_deref(),
                content_disposition: fetched.content_disposition.as_deref(),
                destination_path: None,
                content_hash: None,
                error: None,
            },
        ) {
            let error = format!("could not record download custody: {error}");
            self.events.push(AppEvent::DownloadFailed { node, error });
            return vec![Effect::Redraw];
        }
        self.events.push(AppEvent::DownloadStarted {
            node,
            url: url.clone(),
            bytes: byte_size,
        });
        vec![
            Effect::StoreDownload {
                node,
                url,
                content_type: fetched.content_type,
                content_disposition: fetched.content_disposition,
                received_at_ms,
                bytes: fetched.bytes,
            },
            Effect::Redraw,
        ]
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_download(
        &mut self,
        node: uuid::Uuid,
        url: String,
        content_type: Option<String>,
        content_disposition: Option<String>,
        received_at_ms: u64,
        byte_size: u64,
        result: Result<crate::action::StoredDownload, String>,
    ) -> Vec<Effect> {
        let Some(graph) = self.graph_runtimes.graph_containing_member(node) else {
            return Vec::new();
        };
        let Some(canvas) = self.graph_runtimes.canvas_mut(graph) else {
            return Vec::new();
        };
        if !browse::still_current(canvas, node, &url) {
            return Vec::new();
        }
        let media_type = content_type
            .as_deref()
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        match result {
            Ok(stored) => {
                let hash = stored.content.to_hex();
                let already_attached = canvas
                    .graph()
                    .get_node_by_id(node)
                    .is_some_and(|(_, graph_node)| graph_node.content == Some(stored.content));
                if !already_attached && !canvas.set_node_content_for(node, Some(stored.content)) {
                    let error = "download representation could not attach to its graph node";
                    tracing::warn!(%node, %url, error);
                    let _ = crate::content_classes::set_download_record(
                        canvas,
                        node,
                        crate::content_classes::DownloadFacetRecord {
                            source_url: &url,
                            received_at_ms,
                            byte_size,
                            status: "failed",
                            media_type,
                            content_disposition: content_disposition.as_deref(),
                            destination_path: Some(&stored.destination),
                            content_hash: Some(&hash),
                            error: Some(error),
                        },
                    );
                    self.events.push(AppEvent::DownloadFailed {
                        node,
                        error: error.to_string(),
                    });
                    return vec![Effect::SaveSession, Effect::Redraw];
                }
                if let Err(error) = crate::content_classes::set_download_record(
                    canvas,
                    node,
                    crate::content_classes::DownloadFacetRecord {
                        source_url: &url,
                        received_at_ms,
                        byte_size,
                        status: "completed",
                        media_type,
                        content_disposition: content_disposition.as_deref(),
                        destination_path: Some(&stored.destination),
                        content_hash: Some(&hash),
                        error: None,
                    },
                ) {
                    tracing::warn!(%node, %url, %error, "download metadata could not be completed");
                    self.events.push(AppEvent::DownloadFailed {
                        node,
                        error: error.to_string(),
                    });
                } else {
                    self.events.push(AppEvent::DownloadCompleted {
                        node,
                        destination: stored.destination,
                        content_hash: hash,
                    });
                }
            },
            Err(error) => {
                tracing::warn!(%node, %url, %error, "download storage failed");
                if let Err(facet_error) = crate::content_classes::set_download_record(
                    canvas,
                    node,
                    crate::content_classes::DownloadFacetRecord {
                        source_url: &url,
                        received_at_ms,
                        byte_size,
                        status: "failed",
                        media_type,
                        content_disposition: content_disposition.as_deref(),
                        destination_path: None,
                        content_hash: None,
                        error: Some(&error),
                    },
                ) {
                    tracing::warn!(%node, %facet_error, "download failure metadata could not be recorded");
                }
                self.events.push(AppEvent::DownloadFailed { node, error });
            },
        }
        vec![Effect::SaveSession, Effect::Redraw]
    }
}
