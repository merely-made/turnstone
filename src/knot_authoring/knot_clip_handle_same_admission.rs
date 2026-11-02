// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotClipHandle {
    pub(crate) fn same_admission(&self, other: &Self) -> bool {
        self.same_endpoint(other) && self.expected_base == other.expected_base
    }

    pub(crate) fn same_endpoint(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.hub, &other.hub) && self.target == other.target
    }

    pub(crate) fn keep_status_from(&mut self, previous: &Self) {
        if self.same_endpoint(previous) {
            self.status = previous.status.clone();
        }
    }

    pub fn insert(&self, clip: inker::DocumentClip) -> Result<(), String> {
        let artifacts = clip
            .artifacts
            .into_iter()
            .map(|artifact| KnotClipArtifactV1 {
                role: match artifact.role {
                    inker::DocumentClipArtifactRole::SourceResponse => {
                        KnotClipArtifactRoleV1::SourceResponse
                    },
                    inker::DocumentClipArtifactRole::ObservedRepresentation => {
                        KnotClipArtifactRoleV1::ObservedRepresentation
                    },
                },
                media_type: artifact.media_type,
                canonical_uri: artifact.canonical_uri,
                bytes: artifact.bytes,
            })
            .collect::<Vec<_>>();
        let selectors = clip_selectors(clip.selector.as_deref(), &clip.text, &artifacts);
        let had_dom_range = clip.selector.as_deref().is_some_and(|selector| {
            serde_json::from_str::<serde_json::Value>(selector)
                .ok()
                .and_then(|value| {
                    value
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .as_deref()
                == Some("dom-range")
        });
        let retained_dom_range = selectors
            .iter()
            .any(|selector| matches!(selector, KnotClipSelectorV1::DomRange { .. }));
        let mut fidelity = (!artifacts.is_empty())
            .then_some(KnotClipFidelityV1 {
                class: "arrangement-unchecked".into(),
                detail: "The semantic lowering did not compare computed layout.".into(),
                selector: None,
            })
            .into_iter()
            .collect::<Vec<_>>();
        if had_dom_range && !retained_dom_range {
            fidelity.push(KnotClipFidelityV1 {
                class: if selectors.is_empty() {
                    "selector-unanchored".into()
                } else {
                    "selector-demoted".into()
                },
                detail: if selectors.is_empty() {
                    "The DOM-range selector had no retained observed representation or exact source quote."
                        .into()
                } else {
                    "The DOM-range selector had no retained observed representation; a text quote into the source response is retained instead."
                        .into()
                },
                selector: selectors.first().cloned(),
            });
        }
        let discovered_edges = clip
            .links
            .iter()
            .cloned()
            .map(|target| KnotClipObservedEdgeV1 {
                target,
                relation: "link".into(),
            })
            .collect();
        let fragment = fleece::clip::fragment_from_text(
            clip.source_url.clone(),
            clip.title.clone(),
            clip.text,
            clip.selector.clone(),
            clip.links,
        );
        let pending = PendingClip {
            source_url: clip.source_url,
            title: clip.title,
            selector: clip.selector,
            knot_body: clip_knot_body(&fragment),
            selectors,
            artifacts,
            fidelity,
            discovered_edges,
        };
        *self.status.lock().map_err(|_| "clip status poisoned")? = KnotClipStatus::Sending;
        self.hub
            .commands
            .send(HubCommand::InsertClip {
                expected_base: self.expected_base.clone(),
                address: self.target.clone(),
                clip: pending,
                status: self.status.clone(),
            })
            .map_err(|_| "Knot authoring worker is unavailable".to_string())
    }

    pub fn status(&self) -> KnotClipStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_else(|_| KnotClipStatus::Rejected("clip status unavailable".into()))
    }

    pub fn target(&self) -> &str {
        &self.target
    }
}
