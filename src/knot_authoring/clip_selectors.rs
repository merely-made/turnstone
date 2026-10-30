// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn clip_selectors(
    selector: Option<&str>,
    text: &str,
    artifacts: &[KnotClipArtifactV1],
) -> Vec<KnotClipSelectorV1> {
    let has_observed_representation = artifacts
        .iter()
        .any(|artifact| artifact.role == KnotClipArtifactRoleV1::ObservedRepresentation);
    if has_observed_representation
        && let Some(value) =
            selector.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        && value.get("type").and_then(|v| v.as_str()) == Some("text-position")
        && let (Some(start), Some(end)) = (value["start"].as_u64(), value["end"].as_u64())
    {
        return vec![
            KnotClipSelectorV1::TextPosition {
                artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
                start,
                end,
            },
            KnotClipSelectorV1::TextQuote {
                artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
                exact: text.into(),
                prefix: None,
                suffix: None,
            },
        ];
    }
    let dom_range = has_observed_representation
        .then_some(selector)
        .flatten()
        .and_then(|selector| serde_json::from_str::<serde_json::Value>(selector).ok())
        .and_then(|selector| {
            if selector.get("type")?.as_str()? != "dom-range" {
                return None;
            }
            Some(KnotClipSelectorV1::DomRange {
                artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
                anchor_path: json_path(selector.get("anchor")?.get("path")?)?,
                anchor_offset: selector.get("anchor")?.get("offset")?.as_u64()?,
                focus_path: json_path(selector.get("focus")?.get("path")?)?,
                focus_offset: selector.get("focus")?.get("offset")?.as_u64()?,
                quote: selector
                    .get("quote")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(text)
                    .to_string(),
            })
        });
    dom_range
        .or_else(|| {
            let artifact_role = artifacts.iter().find_map(|artifact| {
                std::str::from_utf8(&artifact.bytes)
                    .ok()
                    .filter(|source| !text.is_empty() && source.contains(text))
                    .map(|_| artifact.role)
            })?;
            Some(KnotClipSelectorV1::TextQuote {
                artifact_role,
                exact: text.to_string(),
                prefix: None,
                suffix: None,
            })
        })
        .into_iter()
        .collect()
}
