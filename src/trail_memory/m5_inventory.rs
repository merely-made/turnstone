// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Independent graph and owner input accounting for the M5 copied-store reader.
use crate::research_search::Member;
use eidetic::records::{
    Evidence, PropertyDefinitionV1, PropertyRecordV1, RecordResult, SourceRecord,
};
use std::collections::{BTreeMap, HashSet};

pub(super) fn capture_evidence(
    admission: &crate::saved_sources::Admission,
    content: &crate::saved_sources::Content,
    selected: bool,
) -> serde_json::Value {
    if selected {
        serde_json::json!({"selected": true, "admission": admission, "content": content})
    } else {
        serde_json::json!({"selected": false, "admission": {"key": admission.key,
        "occurrence": admission.occurrence}, "content_hash": content.text_hash})
    }
}
pub(super) fn note_evidence(
    admission: &serde_json::Value,
    clip: &serde_json::Value,
    text: &str,
    selected: bool,
) -> serde_json::Value {
    if selected {
        serde_json::json!({"selected": true, "admission": admission, "clip": clip, "text": text})
    } else {
        serde_json::json!({"selected": false, "admission": admission})
    }
}

pub(super) fn selection_evidence(
    admission: &serde_json::Value,
    full: serde_json::Value,
    content_hash: &str,
    selected: bool,
) -> serde_json::Value {
    if selected {
        full
    } else {
        serde_json::json!({"selected":false, "admission":admission, "content_hash":content_hash})
    }
}

pub(super) fn output(
    session: uuid::Uuid,
    graph: uuid::Uuid,
    manifest: eidetic::ManifestId,
    result: serde_json::Value,
    records: serde_json::Value,
    captures: Vec<serde_json::Value>,
    note: serde_json::Value,
    selection: serde_json::Value,
    receipt_count: usize,
    receipt_hash: &str,
) -> serde_json::Value {
    serde_json::json!({"schema":"turnstone.m5-store-inspection/v1", "session":session, "graph":graph,
        "manifest":manifest, "result":result, "records":records, "captures":captures,
        "sealed_capture_receipts":{"schema":"turnstone.m5-sealed-capture-receipts/v1",
            "count":receipt_count, "blake3":receipt_hash}, "note":note, "selection":selection})
}

pub(super) fn graph_members(
    graph: &mere::kernel::graph::Graph,
    session: uuid::Uuid,
    graph_id: uuid::Uuid,
    admissions: &[crate::saved_sources::Admission],
    clips: &[crate::research::ClipAdmission],
) -> Result<Vec<Member>, String> {
    let session = crate::panes::SessionId::from_uuid(session);
    let graph_id = crate::panes::GraphId::from_uuid(graph_id);
    let mut captures = HashSet::new();
    let mut expected_clips = Vec::new();
    for (_, node) in graph.nodes() {
        if let Some(value) = graph.facets().get(
            &node.id,
            &chartulary::FacetId::new(crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET),
        ) {
            let values = value.as_array().ok_or("malformed capture references")?;
            for (occurrence, value) in values.iter().enumerate() {
                let annotation_manifest = value
                    .as_str()
                    .ok_or("malformed capture reference")?
                    .to_owned();
                let key = crate::saved_sources::Key {
                    session,
                    graph: graph_id,
                    node: node.id,
                    annotation_manifest,
                };
                if !captures.insert((key, occurrence)) {
                    return Err("duplicate capture occurrence".into());
                }
            }
        }
        if let Some(crate::retained_clip::ClipRequest::Read(manifest)) =
            crate::retained_clip::request(node.url())?
        {
            expected_clips.push(crate::research::ClipAdmission {
                session,
                graph: graph_id,
                node: node.id,
                manifest,
            });
        }
    }
    let observed: HashSet<_> = admissions
        .iter()
        .map(|a| (a.key.clone(), a.occurrence))
        .collect();
    if observed.len() != admissions.len()
        || observed != captures
        || admissions.iter().any(|a| a.error.is_some())
    {
        return Err("capture admission enumeration differs from graph".into());
    }
    expected_clips.sort_by_key(|a| a.node);
    let mut actual_clips = clips.to_vec();
    actual_clips.sort_by_key(|a| a.node);
    if actual_clips != expected_clips {
        return Err("clip admission enumeration differs from graph".into());
    }
    let mut members: Vec<_> = admissions
        .iter()
        .map(|a| Member::Capture(a.key.clone()))
        .collect();
    for clip in clips {
        members.push(Member::Clip(clip.clone()));
    }
    let mut unique = Vec::new();
    for member in members {
        if !unique.contains(&member) {
            unique.push(member);
        }
    }
    Ok(unique)
}

pub(super) fn selected_members(
    value: &serde_json::Value,
    all: &[Member],
) -> Result<Vec<Member>, String> {
    match value["mode"].as_str() {
        Some("all") => Ok(all.to_vec()),
        Some("chosen") => {
            let chosen: Vec<Member> =
                serde_json::from_value(value["members"].clone()).map_err(|e| e.to_string())?;
            let mut seen = Vec::new();
            for member in &chosen {
                if seen.contains(member) || !all.contains(member) {
                    return Err("invalid selected member".into());
                }
                seen.push(member.clone());
            }
            Ok(chosen)
        },
        _ => Err("historical or invalid source selection".into()),
    }
}

pub(super) fn check_snapshot(
    held: &RecordResult,
    sources: Vec<(SourceRecord, String)>,
    records: Vec<(PropertyRecordV1, PropertyDefinitionV1)>,
) -> Result<(), String> {
    if !held.snapshot.complete || !held.snapshot.exclusions.is_empty() {
        return Err("incomplete snapshot".into());
    }
    let mut source_map = BTreeMap::new();
    for (source, text) in sources {
        if let Some(previous) =
            source_map.insert(source.source.clone(), (source.clone(), text.clone()))
        {
            if previous != (source, text) {
                return Err("conflicting source projection".into());
            }
        }
    }
    let mut actual_sources = held.snapshot.sources.clone();
    actual_sources.sort_by(|a, b| a.source.cmp(&b.source));
    if actual_sources
        != source_map
            .values()
            .map(|(s, _)| s.clone())
            .collect::<Vec<_>>()
    {
        return Err("source inventory differs from graph owners".into());
    }
    let mut expected_records = BTreeMap::new();
    let mut definitions = Vec::new();
    for (record, definition) in records {
        let Some((source, text)) = source_map.get(record.evidence.source()) else {
            continue;
        };
        if record.evidence.version() != source.version {
            return Err("record source version differs".into());
        }
        match &record.evidence {
            Evidence::Passage { hash, selector, .. }
                if *hash == eidetic::Hash::of(text.as_bytes())
                    && selector.start < selector.end
                    && selector.end <= text.chars().count() => {},
            _ => return Err("record evidence hash or selector differs".into()),
        }
        if expected_records.insert(record.id.clone(), record).is_some() {
            return Err("duplicate record identity".into());
        }
        if !definitions.contains(&definition) {
            definitions.push(definition);
        }
    }
    let mut actual_records = held.snapshot.records.clone();
    actual_records.sort_by(|a, b| a.id.cmp(&b.id));
    if actual_records != expected_records.into_values().collect::<Vec<_>>() {
        return Err("record inventory differs from graph owners".into());
    }
    let mut actual_defs = held.snapshot.definitions.clone();
    definitions.sort_by_key(|d| (d.id.clone(), d.revision.clone()));
    actual_defs.sort_by_key(|d| (d.id.clone(), d.revision.clone()));
    if actual_defs != definitions {
        return Err("definition inventory differs from graph owners".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use eidetic::models::recall::{PassageSelector, RecallSourceId, RecallSourceKind};
    use eidetic::records::{
        Cardinality, PropertyValue, RecordFilters, RecordOperation, RecordQuery, RecordSettings,
        RecordSnapshot, ValueType,
    };

    #[test]
    fn model_free_inspector_projection_hides_unselected_source_bodies() {
        let key = crate::saved_sources::Key {
            session: crate::panes::SessionId::new(),
            graph: crate::panes::GraphId::new(),
            node: uuid::Uuid::new_v4(),
            annotation_manifest: "sealed-v1:fixture".into(),
        };
        let admission = crate::saved_sources::Admission {
            key,
            occurrence: 0,
            title: "private-title-canary".into(),
            url: "private-url-canary".into(),
            error: None,
        };
        let content = crate::saved_sources::Content {
            title: "private-title-canary".into(),
            origin: "private-url-canary".into(),
            captured_at_ms: 1,
            text: "private-capture-canary".into(),
            text_hash: "hash".into(),
            raw_manifest: "manifest".into(),
        };
        let clip = serde_json::json!({"hidden":"private-clip-canary"});
        let empty = output(
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            eidetic::ManifestId::of_blob(b"test"),
            serde_json::json!({}),
            serde_json::json!({}),
            vec![capture_evidence(&admission, &content, false)],
            note_evidence(
                &serde_json::json!({"id":"note"}),
                &clip,
                "private-note-canary",
                false,
            ),
            selection_evidence(
                &serde_json::json!({"id":"selection"}),
                serde_json::json!({"quote":"private-selection-canary"}),
                "hash",
                false,
            ),
            0,
            "hash",
        );
        let serialized = empty.to_string();
        for canary in [
            "private-title-canary",
            "private-url-canary",
            "private-capture-canary",
            "private-clip-canary",
            "private-note-canary",
            "private-selection-canary",
        ] {
            assert!(!serialized.contains(canary));
        }
        let subset = output(
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            eidetic::ManifestId::of_blob(b"test"),
            serde_json::json!({}),
            serde_json::json!({}),
            vec![capture_evidence(&admission, &content, true)],
            note_evidence(
                &serde_json::json!({"id":"note"}),
                &clip,
                "private-note-canary",
                false,
            ),
            selection_evidence(
                &serde_json::json!({"id":"selection"}),
                serde_json::json!({"quote":"private-selection-canary"}),
                "hash",
                false,
            ),
            0,
            "hash",
        )
        .to_string();
        assert!(subset.contains("private-capture-canary"));
        assert!(!subset.contains("private-note-canary"));
        assert!(!subset.contains("private-clip-canary"));
        let note_only = output(
            uuid::Uuid::nil(),
            uuid::Uuid::nil(),
            eidetic::ManifestId::of_blob(b"test"),
            serde_json::json!({}),
            serde_json::json!({}),
            vec![capture_evidence(&admission, &content, false)],
            note_evidence(
                &serde_json::json!({"id":"note"}),
                &clip,
                "private-note-canary",
                true,
            ),
            selection_evidence(
                &serde_json::json!({"id":"selection"}),
                serde_json::json!({"quote":"private-selection-canary"}),
                "hash",
                false,
            ),
            0,
            "hash",
        )
        .to_string();
        assert!(note_only.contains("private-note-canary"));
        assert!(!note_only.contains("private-capture-canary"));
        assert!(!note_only.contains("private-selection-canary"));
    }

    #[test]
    fn model_free_graph_inventory_counts_occurrences_and_checks_selection() {
        let session = crate::panes::SessionId::from_uuid(uuid::Uuid::new_v4());
        let graph_id = crate::panes::GraphId::from_uuid(uuid::Uuid::new_v4());
        let mut graph = mere::kernel::graph::Graph::new();
        let key = mere::kernel::graph::apply::add_node(
            &mut graph,
            None,
            "https://source.test".into(),
            Default::default(),
        );
        let node = graph.get_node(key).unwrap().id;
        let mut facets = graph.facets().clone();
        crate::source_capture::append_reference(&mut facets, node, "sealed-v1:one".into()).unwrap();
        crate::source_capture::append_reference(&mut facets, node, "sealed-v1:one".into()).unwrap();
        graph.install_facets(facets).unwrap();
        let key = crate::saved_sources::Key {
            session,
            graph: graph_id,
            node,
            annotation_manifest: "sealed-v1:one".into(),
        };
        let admission = |occurrence| crate::saved_sources::Admission {
            key: key.clone(),
            occurrence,
            title: "title".into(),
            url: "https://source.test".into(),
            error: None,
        };
        let admissions = vec![admission(0), admission(1)];
        let members = graph_members(
            &graph,
            *session.as_uuid(),
            *graph_id.as_uuid(),
            &admissions,
            &[],
        )
        .unwrap();
        assert_eq!(members, vec![Member::Capture(key.clone())]);
        assert!(
            graph_members(
                &graph,
                *session.as_uuid(),
                *graph_id.as_uuid(),
                &admissions[..1],
                &[]
            )
            .is_err()
        );
        assert!(
            graph_members(
                &graph,
                *session.as_uuid(),
                *graph_id.as_uuid(),
                &[admission(0), admission(0)],
                &[]
            )
            .is_err()
        );
        assert_eq!(
            selected_members(&serde_json::json!({"mode":"all"}), &members).unwrap(),
            members
        );
        assert!(
            selected_members(
                &serde_json::json!({"mode":"chosen", "members":[]}),
                &members
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            selected_members(
                &serde_json::json!({"mode":"chosen", "members":[members[0], members[0]]}),
                &members
            )
            .is_err()
        );
        assert!(
            selected_members(&serde_json::json!({"mode":"unknown-historical"}), &members).is_err()
        );
    }

    #[test]
    fn model_free_owner_inventory_rejects_omissions_versions_selectors_and_scope() {
        let source = SourceRecord {
            source: RecallSourceId {
                kind: RecallSourceKind::Capture,
                owner: None,
                source: "source".into(),
                node: None,
            },
            version: "v1".into(),
            producer: "capture".into(),
            captured_at: Some(1),
            tags: vec![],
        };
        let definition = PropertyDefinitionV1 {
            id: "p".into(),
            revision: "1".into(),
            value_type: ValueType::Integer,
            unit: None,
            cardinality: Cardinality::Single,
            statuses: vec![],
        };
        let record = PropertyRecordV1 {
            id: "r".into(),
            entity: "e".into(),
            property: "p".into(),
            schema_revision: "1".into(),
            definition_manifest: eidetic::ManifestId::of_blob(b"definition"),
            context: "c".into(),
            value: PropertyValue::Integer(7),
            valid_time: None,
            status: None,
            author: "private".into(),
            method: "manual".into(),
            evidence: Evidence::Passage {
                source: source.source.clone(),
                version: "v1".into(),
                hash: eidetic::Hash::of(b"hello"),
                selector: PassageSelector { start: 0, end: 5 },
            },
            recorded_at: None,
        };
        let mut held = RecordResult {
            operation_version: "v1".into(),
            snapshot: RecordSnapshot {
                scope_generation: "scope".into(),
                complete: true,
                exclusions: vec![],
                sources: vec![source.clone()],
                definitions: vec![definition.clone()],
                records: vec![record.clone()],
                lifecycle: vec![],
            },
            query: RecordQuery {
                operation: RecordOperation::SourceLookup,
                filters: RecordFilters::default(),
            },
            settings: RecordSettings::default(),
            rows: vec![],
        };
        let input = || vec![(source.clone(), "hello".into())];
        let records = || vec![(record.clone(), definition.clone())];
        assert!(check_snapshot(&held, input(), records()).is_ok());
        assert!(check_snapshot(&held, vec![], records()).is_err());
        assert!(check_snapshot(&held, input(), vec![]).is_err());
        let mut bad = record.clone();
        if let Evidence::Passage { version, .. } = &mut bad.evidence {
            *version = "wrong".into();
        }
        assert!(check_snapshot(&held, input(), vec![(bad, definition.clone())]).is_err());
        let mut bad = record.clone();
        if let Evidence::Passage { selector, .. } = &mut bad.evidence {
            selector.end = 6;
        }
        assert!(check_snapshot(&held, input(), vec![(bad, definition.clone())]).is_err());
        let mut bad = record.clone();
        if let Evidence::Passage { hash, .. } = &mut bad.evidence {
            *hash = eidetic::Hash::of(b"wrong");
        }
        assert!(check_snapshot(&held, input(), vec![(bad, definition.clone())]).is_err());
        held.snapshot.sources.clear();
        held.snapshot.records.clear();
        held.snapshot.definitions.clear();
        assert!(check_snapshot(&held, vec![], records()).is_ok());
        assert!(check_snapshot(&held, input(), records()).is_err());
    }
}
