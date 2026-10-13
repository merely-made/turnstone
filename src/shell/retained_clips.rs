// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::{NodeSessions, Shell};
use crate::retained_clip::{ClipRequest, ENGINE_ID, request};

impl Shell {
    /// Mirror decoder/output observations into the retained document view.
    /// This changes presentation only; it never edits the retained source.
    pub(super) fn refresh_retained_clip_playback(&mut self) {
        let preview = self.app.redshank.retained_voice_preview();
        let members: Vec<_> = self
            .app
            .content
            .live_nodes()
            .filter(|member| {
                self.app
                    .content
                    .facts(*member)
                    .is_some_and(|facts| facts.engine == ENGINE_ID)
            })
            .collect();
        for member in members {
            let Some(session) = self.content_sessions.session_mut(&member) else {
                continue;
            };
            let Some(session) = session
                .as_any()
                .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>()
            else {
                continue;
            };
            let document = session.document_mut().document();
            let Ok(Some(ClipRequest::Read(id))) = request(&document.address) else {
                continue;
            };
            let value = preview
                .as_ref()
                .filter(|p| p.id == format!("clip:{}", id.0.to_hex()))
                .map(|p| match &p.state {
                    redshank_playback::PreviewState::Loading => "Loading".to_string(),
                    redshank_playback::PreviewState::Playing => {
                        format!("Playing {} ms", p.position_ms)
                    },
                    redshank_playback::PreviewState::Ended => format!("Ended {} ms", p.position_ms),
                    redshank_playback::PreviewState::Unavailable(reason) => {
                        format!("Unavailable: {reason}")
                    },
                })
                .unwrap_or_else(|| "Ready".into());
            if !document.blocks.iter().any(|b| matches!(b, inker::Block::MetadataRow { label, value: held } if label == "Voice note playback" && *held != value)) { continue; }
            let mut document = document.clone();
            for block in &mut document.blocks {
                if let inker::Block::MetadataRow { label, value: held } = block
                    && label == "Voice note playback"
                {
                    *held = value.clone();
                }
            }
            session.replace_document(document);
        }
    }
}
