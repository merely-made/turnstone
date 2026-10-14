// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use uuid::Uuid;

/// Authority captured at an explicit export action. Completion is useful only
/// while the same owner, access and content session are still shown. Bytes are
/// resolved by Fleece; this lease does not grant storage or disclosure access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipExportScope {
    pub persona: String,
    pub graph_session: Uuid,
    pub access: Uuid,
    pub content_session: u64,
    pub clip: eidetic::ManifestId,
}

impl ClipExportScope {
    pub fn is_current(&self, current: Option<&Self>) -> bool {
        current == Some(self)
    }
    pub fn is_owned_by(&self, owner: Option<&(Uuid, String)>) -> bool {
        owner.is_some_and(|(session, persona)| {
            *session == self.graph_session && persona == &self.persona
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_completion_refuses_changed_persona_session_access_or_clip() {
        let scope = ClipExportScope {
            persona: "alice".into(),
            graph_session: Uuid::new_v4(),
            access: Uuid::new_v4(),
            content_session: 4,
            clip: eidetic::ManifestId::from_hash(eidetic::Hash::of(b"selected")),
        };
        assert!(scope.is_current(Some(&scope)));
        assert!(!scope.is_current(None)); // closed or hidden
        assert!(scope.is_owned_by(Some(&(scope.graph_session, scope.persona.clone()))));
        assert!(!scope.is_owned_by(None)); // owner is temporarily unavailable
        assert!(!scope.is_owned_by(Some(&(Uuid::new_v4(), scope.persona.clone()))));
        assert!(!scope.is_owned_by(Some(&(scope.graph_session, "bob".into()))));
        let mut changed = scope.clone();
        changed.persona = "bob".into();
        assert!(!scope.is_current(Some(&changed)));
        changed = scope.clone();
        changed.graph_session = Uuid::new_v4();
        assert!(!scope.is_current(Some(&changed)));
        changed = scope.clone();
        changed.access = Uuid::new_v4();
        assert!(!scope.is_current(Some(&changed)));
        changed = scope.clone();
        changed.content_session += 1;
        assert!(!scope.is_current(Some(&changed)));
        changed = scope.clone();
        changed.clip = eidetic::ManifestId::from_hash(eidetic::Hash::of(b"other"));
        assert!(!scope.is_current(Some(&changed)));
    }
    #[test]
    fn selected_clip_activation_routes_export_to_its_own_host_without_navigation() {
        use super::super::{ContentHost, ContentProducer};
        use cambium_rootstock::TextureProducer;
        pollster::block_on(async {
            let limits = mere_document_lanes::retained_clip::ClipLimits {
                max_parts: 16,
                max_bytes: 1024 * 1024,
                max_record_bytes: 128 * 1024,
            };
            let context = fleece::portable_clip::CaptureContext {
                source: eidetic::clip_profile::ClipSourceV1 {
                    owner: Some("export-test".into()),
                    entity: "selected".into(),
                    revision: None,
                },
                capture_id: "explicit-export".into(),
                provenance: eidetic::ProvenanceRecord::self_imported("explicit selected body"),
                trust: eidetic::TrustEnvelope::self_asserted(),
                captured_at: eidetic::Timestamp(1),
            };
            let mut store = eidetic::MemoryBackend::new();
            let selected = serde_json::from_str(include_str!(
                "../../../../crates/fleece/tests/fixtures/portable_clip/knot.json"
            ))
            .unwrap();
            let clip = fleece::portable_clip::retain_knot_export(
                &mut store, None, &selected, None, &context, limits,
            )
            .await
            .unwrap();
            let access = Uuid::new_v4();
            let mut host = ContentHost::default();
            let slot = host
                .open_clip(access, &mut store, None, clip, limits)
                .await
                .unwrap();
            host.show_only(&[access]);
            slot.borrow_mut().frame(600., 700., 1.).unwrap();
            let mut producer = ContentProducer::new(slot.clone(), 93);
            let semantics = producer.semantics().unwrap();
            let target = semantics
                .children
                .iter()
                .find(|node| {
                    node.name.starts_with("Export selected clip")
                        && node
                            .actions
                            .iter()
                            .any(|action| action.id.starts_with("activate/"))
                })
                .unwrap();
            let action = &target
                .actions
                .iter()
                .find(|action| action.id.starts_with("activate/"))
                .unwrap()
                .id;
            assert!(producer.act(target.key, action));
            let expected = format!(
                "{}/export",
                mere_document_lanes::retained_clip::address(clip)
            );
            assert!(slot.borrow_mut().take_effects().iter().any(|effect| {
                effect
                    .navigation
                    .as_ref()
                    .is_some_and(|navigation| navigation.request.address == expected)
            }));
            assert_eq!(
                slot.borrow().source().address,
                mere_document_lanes::retained_clip::address(clip)
            );
            assert_eq!(
                slot.borrow().content.address(),
                Some(mere_document_lanes::retained_clip::address(clip).as_str())
            );
            assert!(
                slot.borrow_mut()
                    .content
                    .document_mut()
                    .unwrap()
                    .take_load_commands()
                    .is_empty()
            );
            host.show_only(&[]);
            assert!(!producer.act(target.key, action));
        });
    }
}
