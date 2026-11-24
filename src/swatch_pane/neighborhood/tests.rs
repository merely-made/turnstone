// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn kept_page_replay_restores_repeated_link_evidence_only_for_its_request() {
    use crate::panes::SessionId;
    use crate::saved_sources::Key;

    let generation = Uuid::new_v4();
    let graph = GraphId::from_uuid(Uuid::new_v4());
    let node = Uuid::new_v4();
    let request = Uuid::new_v4();
    let html = r#"<base href="/maps/"><p>One <a href="shared">first map</a></p>
        <p>Two <a href="shared">second map</a></p>"#;
    let document = crate::content::FetchedDocument {
        bytes: html.as_bytes().to_vec(),
        body: html.into(),
        effective_url: Some("https://example.test/effective/source".into()),
        content_type: Some("text/html".into()),
        http_response: None,
        acquired_at_ms: 1,
    };
    let source = KeptSource {
        key: Key {
            session: SessionId::from_uuid(Uuid::new_v4()),
            graph,
            node,
            annotation_manifest: "fixture-manifest".into(),
        },
        request: Uuid::new_v4(),
        capture_hash: crate::source_capture::SourceIdentity::of(&document).hash,
    };
    let mut sources = Sources {
        graph: Some(graph),
        generation: Some(generation),
        ..Sources::default()
    };
    sources.pages.insert(
        node,
        Edition::Kept {
            source: source.clone(),
            request,
            result: None,
        },
    );

    let reply = |generation, request| NeighborhoodReply {
        pane: PaneId(1),
        request,
        generation,
        source: source.clone(),
        result: Ok(extract(&document, &source.capture_hash)),
    };
    sources.accept_reopen(reply(Uuid::new_v4(), request));
    sources.accept_reopen(reply(generation, Uuid::new_v4()));
    assert!(sources.pages[&node].page().is_none());
    sources.accept_reopen(reply(generation, request));
    assert!(sources.changed);

    let live = extract(&document, &source.capture_hash);
    let mut original = Selection {
        focus: document.effective_url.clone(),
        ..Selection::default()
    };
    let mut reopened = Selection {
        focus: original.focus.clone(),
        ..Selection::default()
    };
    project(std::iter::once(&live), None, false, &mut original);
    project(
        sources.pages.values().filter_map(Edition::page),
        None,
        false,
        &mut reopened,
    );
    assert_eq!(
        original.details.keys().collect::<Vec<_>>(),
        reopened.details.keys().collect::<Vec<_>>()
    );
    let links = &reopened.details.values().next().unwrap().observations;
    assert_eq!(
        links,
        &original.details.values().next().unwrap().observations
    );
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].to, "https://example.test/maps/shared");
    assert_ne!(links[0].locator, links[1].locator);
}

#[test]
fn edges_and_nodes_are_clickable_in_narrow_and_wide_swatches() {
    use crate::swatch_pane::{GraphCanvasNode, SWATCH_PAD, SwatchPane};

    let center = Uuid::from_u128(1);
    let right = Uuid::from_u128(2);
    let left = Uuid::from_u128(3);
    let spokes = [("right", right), ("left", left)];
    let mut pane = SwatchPane::new(RESOURCE_NEIGHBORHOOD);
    pane.runner.update(|state| {
        state.swatch.selected = Some(center);
        state.swatch.graph.nodes = [(center, 0.5), (right, 0.86), (left, 0.14)]
            .into_iter()
            .map(|(id, x)| GraphCanvasNode {
                id,
                kind: NodeState::Idle,
                position: (x, 0.5),
                label: id.to_string(),
                key: Some(id.to_string()),
            })
            .collect();
        for (id, to) in spokes {
            state.swatch.relations.push(cambium::GraphCanvasRelation {
                id: id.into(),
                from: center,
                to,
                kind: "resource-reference".into(),
                label: "Link observation".into(),
                route: Vec::new(),
                visible: true,
                emphasized: false,
                link_key: None,
                marks: Default::default(),
                strength: 1.0,
            });
            state.neighborhood.as_mut().unwrap().details.insert(
                id.into(),
                ObservedLinkInspection {
                    target: LinkTarget::new(center, to),
                    observations: Vec::new(),
                    total: 0,
                },
            );
        }
    });

    for width in [150, 424] {
        pane.runner.update(|state| {
            state.viewport_w = width as f32;
            state.viewport_h = 224.0;
            state.swatch.width = width - 2 * SWATCH_PAD as u32;
            state.swatch.height = 200;
            fit_node_targets(&mut state.swatch);
        });
        let _ = pane.scene(width, 224, 1.0);
        let positions: Vec<_> = pane
            .runner
            .state()
            .swatch
            .projected_positions()
            .into_iter()
            .map(|(_, point)| point)
            .collect();
        let (center_x, y) = positions[0];
        let right_x = positions[1].0;
        let left_x = positions[2].0;
        for (id, x) in [("right", right_x), ("left", left_x)] {
            pane.click((center_x + x) / 2.0 + SWATCH_PAD, y + SWATCH_PAD, width, 224);
            let selection = pane.runner.state().neighborhood.as_ref().unwrap();
            assert_eq!(selection.link.as_deref(), Some(id), "width {width}");
        }
        pane.click(right_x + SWATCH_PAD, y + SWATCH_PAD, width, 224);
        let selection = pane.runner.state().neighborhood.as_ref().unwrap();
        assert_eq!(selection.focus, Some(right.to_string()));
        assert_eq!(selection.link, None);
    }
}

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
        http_response: None,
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
