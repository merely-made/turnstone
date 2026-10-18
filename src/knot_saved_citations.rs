// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The product's private-reader navigation over Knot-owned citation records.
//! No origin fetch, new evidence format, or alternate text resolver.
use chirograph::{KnotClipArtifactRoleV1, KnotClipSelectorV1};

pub(crate) fn controls(source: &str) -> Result<Vec<(String, String)>, String> {
    let mut controls = Vec::new();
    for citation in knot::clip_citations(source.as_bytes())? {
        if !citation.source_url.starts_with("mere://saved-source/") {
            continue;
        }
        let target = crate::saved_sources::parse_address(&citation.source_url)?
            .ok_or("The saved citation has no private reader identity.")?;
        let range = target
            .selector
            .as_ref()
            .ok_or("The saved citation has no exact range.")?;
        let mut whole = target.clone();
        whole.selector = None;
        let canonical = crate::saved_sources::address(&whole)?;
        let references: Vec<_> = citation
            .evidence
            .iter()
            .filter(|r| {
                r.role == KnotClipArtifactRoleV1::ObservedRepresentation
                    && r.canonical_uri == canonical
                    && format!("blake3:{}", r.digest) == target.text_hash
            })
            .collect();
        let [reference] = references.as_slice() else {
            return Err(
                "The saved citation's artifact identity differs from its reader target.".into(),
            );
        };
        reference.blob_hash()?;
        let positions: Vec<_> = citation
            .selectors
            .iter()
            .filter_map(|s| match s {
                KnotClipSelectorV1::TextPosition {
                    artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
                    start,
                    end,
                } => Some((*start, *end)),
                _ => None,
            })
            .collect();
        if positions != vec![(range.start as u64, range.end as u64)] {
            return Err("The saved citation's scalar range differs from its reader target.".into());
        }
        controls.push((
            format!(
                "Read saved passage {}: {}",
                controls.len() + 1,
                citation.title.as_deref().unwrap_or("Saved source")
            ),
            citation.source_url,
        ));
    }
    Ok(controls)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use knot::KnotClipEvidenceStore;

    pub(crate) fn fixture() -> (String, crate::saved_sources::Target) {
        let text = "café · 🐦 · café";
        let target = crate::saved_sources::Target {
            key: crate::saved_sources::Key {
                session: crate::panes::SessionId::new(),
                graph: crate::panes::GraphId::new(),
                node: uuid::Uuid::new_v4(),
                annotation_manifest: eidetic::Hash::of(b"A1").to_string(),
            },
            text_hash: eidetic::Hash::of(text.as_bytes()).to_string(),
            selector: Some(crate::saved_sources::Selector { start: 11, end: 15 }),
        };
        let content = crate::saved_sources::Content {
            title: "A1".into(),
            origin: "https://gone.test/A".into(),
            captured_at_ms: 42,
            text: text.into(),
            text_hash: target.text_hash.clone(),
            raw_manifest: eidetic::Hash::of(b"raw").to_string(),
        };
        let clip = crate::saved_sources::clip(&content, &target).unwrap();
        assert_eq!(clip.text, "café");
        assert!(clip.links.is_empty());
        let temp = tempfile::tempdir().unwrap();
        let mut store = knot::FileClipEvidenceStore::new(temp.path(), 4096);
        let reference = store
            .retain(&chirograph::KnotClipArtifactV1 {
                role: KnotClipArtifactRoleV1::ObservedRepresentation,
                media_type: clip.artifacts[0].media_type.clone(),
                canonical_uri: clip.artifacts[0].canonical_uri.clone(),
                bytes: clip.artifacts[0].bytes.clone(),
            })
            .unwrap();
        let value = serde_json::json!({"schema":chirograph::KNOT_CLIP_INSERT_SCHEMA_V2,"source_url":clip.source_url,"title":clip.title,"selectors":[KnotClipSelectorV1::TextPosition {artifact_role:KnotClipArtifactRoleV1::ObservedRepresentation,start:11,end:15},KnotClipSelectorV1::TextQuote {artifact_role:KnotClipArtifactRoleV1::ObservedRepresentation,exact:"café".into(),prefix:None,suffix:None}],"evidence":[reference]});
        (
            format!("# My authored observation\n\n```knot.clip.provenance\n{value}\n```\n\ncafé\n"),
            target,
        )
    }

    #[test]
    fn saved_citation_control_binds_artifact_and_exact_range() {
        let (source, target) = fixture();
        let expected = crate::saved_sources::address(&target).unwrap();
        assert_eq!(
            controls(&source).unwrap(),
            vec![("Read saved passage 1: A1".into(), expected.clone())]
        );
        assert_eq!(
            knot::clip_citations(source.as_bytes()).unwrap()[0]
                .read_passage(0, "café · 🐦 · café".as_bytes())
                .unwrap(),
            "café"
        );
        for tampered in [
            source.replace("start=11", "start=10"),
            source.replace("\"start\":11", "\"start\":10"),
            source.replace(&expected, &expected.replace("start=11", "start=10")),
        ] {
            assert!(controls(&tampered).is_err());
        }
        assert!(
            controls(&source.replace("```knot.clip.provenance\n", "```knot.clip.provenance\n{"))
                .is_err()
        );
    }
}
