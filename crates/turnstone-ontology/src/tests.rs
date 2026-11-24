// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use redshank_model::{
    AnnotationId, CaptureAnchor, ItemId, LibraryItem, MediaSource, NotePrivacy, RedshankModel,
    RepresentationReceipt,
};

fn export() -> Vec<u8> {
    let mut model = RedshankModel::default();
    let item = ItemId("rehearsal".into());
    let note = AnnotationId("turnaround".into());
    model
        .add_item(LibraryItem::DirectAudio {
            id: item.clone(),
            title: "Rehearsal".into(),
            source: MediaSource::Enclosure {
                url: "https://example.org/take.wav".into(),
            },
        })
        .unwrap();
    model
        .add_span_annotation(
            note.clone(),
            CaptureAnchor {
                item_id: item.clone(),
                offset_ms: 12_500,
                end_offset_ms: None,
                pressed_offset_ms: None,
                fingerprint: None,
                representation: RepresentationReceipt::default(),
            },
            14_250,
            "second turnaround".into(),
            1,
        )
        .unwrap();
    model
        .set_annotation_privacy(&note, NotePrivacy::Shareable)
        .unwrap();
    let lens = redshank_model::ontology::load(None).unwrap();
    serde_json::to_vec(&model.export_annotations_with_profile(&item, &lens)).unwrap()
}

#[test]
fn redshank_export_opens_the_recorded_passage_and_reopens_offline() {
    let notes = passages(&load_profile(None).unwrap(), &export()).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title.as_deref(), Some("second turnaround"));
    assert_eq!(
        notes[0].label(),
        "second turnaround · 12.500 to 14.250 s · https://example.org/take.wav"
    );
    let mut untitled = notes[0].clone();
    untitled.title = None;
    assert!(
        untitled
            .label()
            .starts_with("Open passage · 12.500 to 14.250 s")
    );
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("listening-note.json");
    std::fs::write(&file, export()).unwrap();
    for _ in 0..2 {
        let profile = load_profile(None).unwrap();
        let result = open_annotation(&profile, &file, "mere://redshank/note/turnaround").unwrap();
        assert_eq!(
            result.as_str(),
            "https://example.org/take.wav#t=12.500,14.250"
        );
    }
    let lens = include_str!("../packs/lens.toml")
        .lines()
        .filter(|line| !line.contains("pack:web-annotation"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(directory.path().join("lens.toml"), lens).unwrap();
    let without = load_profile(Some(directory.path())).unwrap();
    assert!(open_annotation(&without, &file, "mere://redshank/note/turnaround").is_err());
}

#[test]
fn an_ambiguous_target_or_malformed_selector_never_becomes_an_action() {
    let profile = load_profile(None).unwrap();
    let page: serde_json::Value = serde_json::from_slice(&export()).unwrap();
    for invalid in ["t=15.0,12.0", "xywh=1,2,3,4", "t=NaN", "t=12.5&other=1"] {
        let mut copy = page.clone();
        copy["items"][0]["target"]["selector"]["value"] = invalid.into();
        assert!(
            passages(&profile, &serde_json::to_vec(&copy).unwrap())
                .unwrap()
                .is_empty()
        );
    }
    let mut copy = page.clone();
    let target = &page["items"][0]["target"];
    copy["items"][0]["target"] =
        serde_json::json!([target, {"source":"https://example.org/other"}]);
    assert!(
        passages(&profile, &serde_json::to_vec(&copy).unwrap())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn web_notes_cannot_open_local_schemes_or_change_existing_fragments() {
    let profile = load_profile(None).unwrap();
    let mut page: serde_json::Value = serde_json::from_slice(&export()).unwrap();
    for target in [
        "file:///tmp/other.wav",
        "mere://private/record",
        "https://example.org/take.wav#part",
    ] {
        page["items"][0]["target"]["source"] = target.into();
        assert!(
            passages(&profile, &serde_json::to_vec(&page).unwrap())
                .unwrap()
                .is_empty()
        );
    }
    page["@context"] = "https://unavailable.example/custom-context".into();
    assert!(passages(&profile, &serde_json::to_vec(&page).unwrap()).is_err());
}

#[test]
fn named_records_follow_selected_predicates_and_types_with_document_scope() {
    let data = br#"{
      "@graph": [
        {"@id":"urn:verse", "http://purl.org/spar/cito/cites":{"@id":"urn:take"},
         "http://www.w3.org/2000/01/rdf-schema#label":"Verse one"},
        {"@id":"urn:take", "@type":"http://www.w3.org/ns/oa#TextualBody"},
        {"@id":"urn:unknown","urn:unknown:rel":{"@id":"urn:take"}},
        {"@id":"urn:separate","@graph":[{"@id":"urn:private","http://purl.org/spar/cito/cites":{"@id":"urn:take"}}]}]
    }"#;
    let profile = load_profile(None).unwrap();
    let result = read(&profile, data).unwrap();
    assert!(result.passages.is_empty());
    assert_eq!(result.name("urn:verse"), "Verse one");
    assert_eq!(result.links.records().len(), 2);
    assert_eq!(
        result.links.links("urn:take").next().unwrap().subject,
        "urn:verse"
    );
    assert_eq!(
        result.links.members("http://www.w3.org/ns/oa#TextualBody"),
        ["urn:take"]
    );
    assert_eq!(result.links.types("urn:take").len(), 1);
    let empty = tempfile::tempdir().unwrap();
    std::fs::write(
        empty.path().join("lens.toml"),
        include_str!("../packs/lens.toml")
            .split("[lens]")
            .next()
            .unwrap()
            .to_owned()
            + "[lens]\nselect = []\n",
    )
    .unwrap();
    assert!(
        read(&load_profile(Some(empty.path())).unwrap(), data)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn the_same_selection_preserves_source_and_millisecond_bounds_for_other_actions() {
    let result = read(&load_profile(None).unwrap(), &export()).unwrap();
    assert_eq!(
        result.passages[0].time_range_ms(),
        Some((12500, Some(14250)))
    );
    assert_eq!(
        result.passages[0].source_address().as_str(),
        "https://example.org/take.wav"
    );
}

#[test]
fn relative_turtle_claims_resolve_against_the_response_address() {
    let turtle = b"<verse> <http://purl.org/spar/cito/cites> <take> .";
    let doc = inline_reading(
        Some("text/turtle; charset=utf-8"),
        turtle,
        Some("https://local.test/revision/notes.ttl"),
    );
    assert_eq!(
        doc.links
            .links("https://local.test/revision/take")
            .next()
            .unwrap()
            .subject,
        "https://local.test/revision/verse"
    );
    assert!(inline_reading(Some("text/plain"), turtle, None).is_empty());
}

#[test]
fn repeated_record_titles_use_the_class_then_resource_identity() {
    let profile = crate::load_profile(None).unwrap();
    let graph = br#"{"@context":{"label":"http://www.w3.org/2000/01/rdf-schema#label","oa":"http://www.w3.org/ns/oa#"},"@graph":[
        {"@id":"urn:work","@type":"oa:TextualBody","label":"One"},
        {"@id":"urn:reading:a","@type":"oa:Annotation","label":"One"},
        {"@id":"urn:reading:b","@type":"oa:Annotation","label":"One"}
    ]}"#;
    let result = crate::read(&profile, graph).unwrap();
    let work = result.name("urn:work");
    assert!(work.starts_with("One · ") && !work.contains("urn:"));
    assert!(result.name("urn:reading:a").ends_with(" · urn:reading:a"));
    assert!(result.name("urn:reading:b").ends_with(" · urn:reading:b"));
}
