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

    let passage = Passage::from_clip(
        Uuid::new_v4(),
        address,
        Uuid::new_v4(),
        inker::DocumentClip {
            source_url: address.into(),
            title: None,
            text: "Town's name".into(),
            selector: Some("selected range".into()),
            links: vec!["town".into()],
            artifacts: vec![inker::DocumentClipArtifact {
                role: inker::DocumentClipArtifactRole::ObservedRepresentation,
                media_type: "text/html".into(),
                canonical_uri: address.into(),
                bytes: html.as_bytes().into(),
            }],
        },
    )
    .expect("a selection with an observed representation");
    let mut selection = Selection {
        focus: Some(address.into()),
        ..Selection::default()
    };
    let model = project([&page].into_iter(), Some(&passage), false, &mut selection);
    assert_eq!(model.nodes.len(), 2);
    assert_eq!(model.relations.len(), 1);
    assert!(selection.details[&model.relations[0].id()][0].contains("Selected passage"));

    selection.focus = Some(resource.into());
    let model = project([&page].into_iter(), Some(&passage), false, &mut selection);
    assert_eq!(model.selected, Some(node_id(resource)));
    assert!(
        model
            .nodes
            .iter()
            .any(|node| node.key.as_deref() == Some(related))
    );
    let evidence: Vec<_> = selection.details.values().flatten().collect();
    assert!(evidence.iter().any(|row| row.contains("Town's name")));
    assert!(evidence.iter().any(|row| row.contains("Town's map")));
    assert!(
        evidence
            .iter()
            .any(|row| row.contains("https://schema.org/containedInPlace"))
    );
}
