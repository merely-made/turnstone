// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn moving_from_a_passage_to_a_resource_exposes_its_other_sources() {
    let address = "https://example.test/article";
    let resource = "https://example.test/places/town";
    let related = "https://example.test/places/region";
    let html = r#"
        <base href="/places/">
        <a href="town">Town's name</a>
        <a href="town">Town's map</a>
        <a href="unrelated">Outside the selection</a>
        <script type="application/ld+json">
        {"@id": "town", "https://schema.org/containedInPlace": {"@id": "region"}}
        </script>
    "#;
    let document = crate::content::FetchedDocument {
        bytes: html.as_bytes().into(),
        body: html.into(),
        content_type: Some("text/html".into()),
        effective_url: Some(address.into()),
        acquired_at_ms: 1,
    };
    let page = extract(&document, "capture");
    assert!(!page.partial);
    assert!(
        page.claims.iter().any(|claim| {
            claim.from == resource && claim.to == related && claim.source == address
        })
    );

    let identity = crate::source_capture::SourceIdentity::of(&document);
    let clip = inker::DocumentClip {
        source_url: address.into(),
        title: None,
        text: "Town's map".into(),
        selector: Some(r#"{"type":"dom-range","version":1,"link_occurrence_orders":[1]}"#.into()),
        links: vec!["town".into()],
        artifacts: vec![inker::DocumentClipArtifact {
            role: inker::DocumentClipArtifactRole::SourceResponse,
            media_type: "text/html".into(),
            canonical_uri: address.into(),
            bytes: document.bytes.clone(),
        }],
    };
    let passage = Passage::from_clip(
        Uuid::new_v4(),
        &document,
        &identity,
        Uuid::new_v4(),
        clip.clone(),
    )
    .expect("a selection from the admitted response");
    let mut selection = Selection {
        focus: Some(address.into()),
        ..Selection::default()
    };
    let model = project([&page].into_iter(), Some(&passage), false, &mut selection);
    assert_eq!(model.nodes.len(), 2);
    assert_eq!(model.relations.len(), 1);
    let inspection = &selection.details[&model.relations[0].id()];
    assert_eq!(inspection.total, 1);
    assert_eq!(inspection.details()[0].title, "HTML link 2");
    assert!(
        inspection.details()[0]
            .rows
            .iter()
            .any(|row| { row.key == "Context" && row.value.contains("[Town's map]") })
    );

    selection.focus = Some(resource.into());
    let model = project([&page].into_iter(), Some(&passage), false, &mut selection);
    assert_eq!(model.selected, Some(node_id(resource)));
    assert!(
        model
            .nodes
            .iter()
            .any(|node| node.key.as_deref() == Some(related))
    );
    let evidence: Vec<_> = selection
        .details
        .values()
        .flat_map(|link| link.details())
        .collect();
    assert!(
        evidence
            .iter()
            .any(|section| section.title == "HTML link 1")
    );
    assert!(
        evidence
            .iter()
            .any(|section| section.title == "HTML link 2")
    );
    assert!(
        evidence
            .iter()
            .flat_map(|section| &section.rows)
            .any(|row| row.value == "https://schema.org/containedInPlace")
    );

    let mut stale = clip;
    stale.artifacts[0].bytes.push(b' ');
    assert!(
        Passage::from_clip(Uuid::new_v4(), &document, &identity, Uuid::new_v4(), stale,).is_none()
    );
}
