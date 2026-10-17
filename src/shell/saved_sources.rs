// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Local document session for host-resolved private saved-source text.
//!
//! The engine accepts only a canonical internal identity and a packet already
//! resolved by the shell. It has no fetcher and cannot open the original URL.

use inker::{DocumentSession, EngineDocument, SessionEngine, SessionError, SessionSpawnRequest};
use netrender::Scene;

pub(crate) struct SavedSourceSessionEngine;

impl SessionEngine<Scene> for SavedSourceSessionEngine {
    fn engine_id(&self) -> &str {
        crate::saved_sources::ENGINE_ID
    }

    fn spawn(
        &self,
        request: &SessionSpawnRequest,
    ) -> Result<Box<dyn DocumentSession<Scene>>, SessionError> {
        let target = crate::saved_sources::parse_address(&request.address)
            .map_err(SessionError::SpawnFailed)?
            .ok_or_else(|| {
                SessionError::SpawnFailed("saved source address is not a private document".into())
            })?;
        let body = request.body.as_deref().ok_or_else(|| {
            SessionError::Unsupported("saved source requires host-resolved retained text".into())
        })?;
        let document: EngineDocument = serde_json::from_str(body)
            .map_err(|error| SessionError::SpawnFailed(error.to_string()))?;
        if document.address != request.address
            || document.address
                != crate::saved_sources::address(&target).map_err(SessionError::SpawnFailed)?
            || document.provenance.source_kind.as_deref() != Some(crate::saved_sources::ENGINE_ID)
            || document.provenance.canonical_uri.as_deref() != Some(request.address.as_str())
            || document.content_type != "text/plain; profile=saved-source"
        {
            return Err(SessionError::SpawnFailed(
                "saved source presentation identity differs".into(),
            ));
        }
        let doc = mere_document_lanes::SmolwebDocument::from_document_with_theme(
            document,
            mere_document_lanes::SmolwebTheme::System,
        );
        Ok(Box::new(mere_document_lanes::SmolwebDocumentSession::new(
            doc,
            request.viewport,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saved_sources::{Content, Key, Selector, Target};

    fn saved() -> (String, EngineDocument) {
        let content = Content {
            title: "Private evidence".into(),
            origin: "https://gone.test/original".into(),
            captured_at_ms: 1000,
            text: "old 🐦 retained words".into(),
            text_hash: eidetic::Hash::of("old 🐦 retained words".as_bytes()).to_string(),
            raw_manifest: eidetic::Hash::of(b"raw response").to_string(),
        };
        let target = Target {
            key: Key {
                session: crate::panes::SessionId::new(),
                graph: crate::panes::GraphId::new(),
                node: uuid::Uuid::new_v4(),
                annotation_manifest: eidetic::Hash::of(b"annotation").to_string(),
            },
            text_hash: content.text_hash.clone(),
            selector: Some(Selector { start: 4, end: 5 }),
        };
        let document = crate::saved_sources::document(&content, &target).unwrap();
        (document.address.clone(), document)
    }

    #[test]
    fn only_host_resolved_local_identity_spawns_the_real_retained_session() {
        let (address, document) = saved();
        let request = SessionSpawnRequest::new(&address).with_viewport(700, 500);
        assert!(SavedSourceSessionEngine.spawn(&request).is_err());
        let request = request.with_body(serde_json::to_string(&document).unwrap());
        let mut session = SavedSourceSessionEngine.spawn(&request).unwrap();
        let retained = session
            .as_any()
            .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
            .expect("same retained session used for document rendering");
        assert_eq!(retained.document_mut().document(), &document);
        let original = SessionSpawnRequest::new("https://gone.test/original")
            .with_body(serde_json::to_string(&document).unwrap());
        assert!(SavedSourceSessionEngine.spawn(&original).is_err());
    }

    #[test]
    fn another_document_or_forged_presentation_identity_is_refused() {
        let (address, mut document) = saved();
        document.provenance.canonical_uri = Some("https://gone.test/original".into());
        let request =
            SessionSpawnRequest::new(&address).with_body(serde_json::to_string(&document).unwrap());
        assert!(SavedSourceSessionEngine.spawn(&request).is_err());
        let (_, other) = saved();
        let request =
            SessionSpawnRequest::new(&address).with_body(serde_json::to_string(&other).unwrap());
        assert!(SavedSourceSessionEngine.spawn(&request).is_err());
    }
}
