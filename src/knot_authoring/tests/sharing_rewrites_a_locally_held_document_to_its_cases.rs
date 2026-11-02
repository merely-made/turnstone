// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn sharing_rewrites_a_locally_held_document_to_its_holder() {
    let temp = tempfile::tempdir().unwrap();
    let vault = temp.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).unwrap();
    let document = vault.join("notes").join("field.knot");
    std::fs::write(&document, "# Field\n").unwrap();
    let root = [0x11; 32];

    let rewritten = place_held_rewrite(&file_address(&document), &vault, &root)
        .expect("a document inside the vault rewrites");
    assert_eq!(
        parse_place_held_address(&rewritten),
        Some((root, "notes/field.knot".to_string()))
    );
    // The resident vault form rewrites by the same rule.
    assert_eq!(
        place_held_rewrite("knot://vault/field-note", &vault, &root),
        Some(place_held_address(&root, "field-note"))
    );
    // Already place-held: left alone, so re-sharing cannot re-attribute
    // somebody else's document to whoever passed it along.
    assert_eq!(place_held_rewrite(&rewritten, &vault, &root), None);
    // Outside the vault: not this mere's to offer.
    let outside = temp.path().join("elsewhere.knot");
    std::fs::write(&outside, "# Elsewhere\n").unwrap();
    assert_eq!(
        place_held_rewrite(&file_address(&outside), &vault, &root),
        None
    );
}

#[test]
fn a_place_held_address_matches_the_holder_own_disclosed_address() {
    let requested = place_held_address(&[0x22; 32], "notes/field.knot");
    // The holder discloses its own local path, which the visitor has no
    // way to know; the path inside the vault is what both ends agree on.
    assert!(address_matches(
        &requested,
        "file:///C:/someone-else/vault/notes/field.knot"
    ));
    assert!(!address_matches(
        &requested,
        "file:///C:/vault/other/field.knot"
    ));
    assert!(!address_matches(&requested, "notes/field.knot"));
    // An ordinary address is still matched exactly and nothing else.
    assert!(address_matches("knot://vault/one", "knot://vault/one"));
    assert!(!address_matches("knot://vault/one", "knot://vault/one-two"));
}

#[test]
fn dom_ranges_only_name_an_observed_representation() {
    let dom_range = serde_json::json!({
        "type": "dom-range",
        "version": 1,
        "anchor": { "path": [0, 1], "offset": 0 },
        "focus": { "path": [0, 1], "offset": 17 },
        "quote": "A useful finding."
    })
    .to_string();
    let source = KnotClipArtifactV1 {
        role: KnotClipArtifactRoleV1::SourceResponse,
        media_type: "text/html".into(),
        canonical_uri: "https://example.test/report".into(),
        bytes: b"<p>A useful finding.</p>".to_vec(),
    };
    assert!(matches!(
        clip_selectors(Some(&dom_range), "A useful finding.", &[source]).as_slice(),
        [KnotClipSelectorV1::TextQuote {
            artifact_role: KnotClipArtifactRoleV1::SourceResponse,
            ..
        }]
    ));

    let observed = KnotClipArtifactV1 {
        role: KnotClipArtifactRoleV1::ObservedRepresentation,
        media_type: "application/vnd.mere.dom+json".into(),
        canonical_uri: "https://example.test/report".into(),
        bytes: br#"{"node":"p","text":"A useful finding."}"#.to_vec(),
    };
    assert!(matches!(
        clip_selectors(Some(&dom_range), "A useful finding.", &[observed]).as_slice(),
        [KnotClipSelectorV1::DomRange {
            artifact_role: KnotClipArtifactRoleV1::ObservedRepresentation,
            ..
        }]
    ));
}

#[test]
#[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
fn real_knot_consumer_saves_rejects_stale_and_reopens() {
    let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
        .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
    let root =
        std::env::temp_dir().join(format!("turnstone-knot-authoring-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("field.knot");
    fs::write(&path, "# Field\n").unwrap();
    let address = file_address(&path);

    {
        let engine = KnotAuthoringEngine::connect_directory(program.clone(), &root, 4096).unwrap();
        let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
        let mut first = engine.spawn(&request).unwrap();
        let mut second = engine.spawn(&request).unwrap();
        let first = first
            .as_any()
            .downcast_mut::<KnotDocumentSession>()
            .unwrap();
        let second = second
            .as_any()
            .downcast_mut::<KnotDocumentSession>()
            .unwrap();

        focus_editor(first);
        type_text(first, "discard");
        assert!(first.dispatch_key(KeyEvent::with_mods(
            Key::Character("z".into()),
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        )));
        assert_eq!(first.runner.state().editor.source(), "# Field\n");
        type_text(first, "First ");
        let before_preedit = first.runner.state().editor.source().to_string();
        assert!(
            first.dispatch_key(KeyEvent::new(Key::Composition(CompositionEvent::Preedit {
                text: "かな".into(),
                selection: Some((3, 3)),
            },)))
        );
        assert_eq!(first.runner.state().editor.source(), before_preedit);
        assert!(
            first.dispatch_key(KeyEvent::new(Key::Composition(CompositionEvent::Commit(
                "仮名".into()
            ),)))
        );

        focus_editor(second);
        type_text(second, "Second ");
        assert_eq!(second.status(), "unsaved");

        save_shortcut(first);
        wait_for_status(first, "saved");
        wait_for_status(second, "stale");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# Field\nFirst 仮名");

        save_shortcut(second);
        wait_for_status(second, "stale");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# Field\nFirst 仮名",
            "the stale local buffer must not overwrite the accepted save"
        );

        second.reload();
        wait_for_status(second, "saved");
        assert_eq!(second.runner.state().editor.source(), "# Field\nFirst 仮名");
        focus_editor(second);
        type_text(second, " after churn");
        let previous_refreshes = second.revision_refreshes;
        fs::write(root.join("other.knot"), "# Other\n").unwrap();
        wait_for_refresh(second, previous_refreshes);
        assert_eq!(
            second.status(),
            "unsaved",
            "an unrelated revision must retain the local buffer"
        );
        save_shortcut(second);
        wait_for_status(second, "saved");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# Field\nFirst 仮名 after churn"
        );
    }

    {
        let engine = KnotAuthoringEngine::connect_directory(program, &root, 4096).unwrap();
        let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
        let mut reopened = engine.spawn(&request).unwrap();
        let reopened = reopened
            .as_any()
            .downcast_mut::<KnotDocumentSession>()
            .unwrap();
        assert_eq!(
            reopened.runner.state().editor.source(),
            "# Field\nFirst 仮名 after churn"
        );
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
fn inspector_clip_crosses_the_typed_endpoint_action() {
    let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
        .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
    let root = std::env::temp_dir().join(format!("turnstone-knot-clip-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("field.knot");
    fs::write(&path, "# Field\n").unwrap();
    let address = file_address(&path);

    let engine = KnotAuthoringEngine::connect_directory(program, &root, 4096).unwrap();
    let handle = KnotClipHandle {
        expected_base: None,
        hub: engine
            .hub
            .clone()
            .expect("a connected fixture has a local hub"),
        target: address,
        status: Arc::new(Mutex::new(KnotClipStatus::Ready)),
    };
    handle
        .insert(inker::DocumentClip {
            source_url: "https://example.test/report".into(),
            title: Some("The report".into()),
            text: "A useful finding.".into(),
            selector: None,
            links: vec!["https://example.test/source".into()],
            artifacts: Vec::new(),
        })
        .unwrap();
    wait_for_clip_status(&handle, KnotClipStatus::Saved);

    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains("```knot.clip.provenance"));
    assert!(saved.contains(r#""source_url":"https://example.test/report""#));
    assert!(saved.contains("# The report"));
    assert!(saved.contains("A useful finding."));
    assert!(saved.contains("https://example.test/source"));

    fs::remove_dir_all(root).unwrap();
}

#[cfg(feature = "peers")]
#[test]
#[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
fn inspector_clip_retains_livery_source_evidence_end_to_end() {
    let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
        .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
    let root =
        std::env::temp_dir().join(format!("turnstone-knot-clip-v2-{}", uuid::Uuid::new_v4()));
    let evidence =
        std::env::temp_dir().join(format!("turnstone-knot-evidence-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    fs::create_dir(&evidence).unwrap();
    let path = root.join("field.djot");
    fs::write(&path, "# Field\n").unwrap();
    let address = file_address(&path);
    let html = "<article><h1>The report</h1><p>A useful finding.</p>\
                    <a href=\"https://example.test/source\">Source</a></article>";
    let source_engine = genet_documents::LiverySessionEngine::new(genet_documents::LocalFetcher);
    let source_request = SessionSpawnRequest::new("https://example.test/report").with_body(html);
    let source_clip = source_engine
        .spawn(&source_request)
        .unwrap()
        .clip()
        .expect("the Livery session should lower its document");
    let bytes = html.as_bytes().to_vec();
    let digest = blake3::hash(&bytes).to_hex().to_string();

    let engine =
        KnotAuthoringEngine::connect_directory_evidence(program, &root, 4096, &evidence, 4096)
            .unwrap();
    let handle = KnotClipHandle {
        expected_base: None,
        hub: engine
            .hub
            .clone()
            .expect("a connected fixture has a local hub"),
        target: address,
        status: Arc::new(Mutex::new(KnotClipStatus::Ready)),
    };
    handle.insert(source_clip).unwrap();
    wait_for_clip_status(&handle, KnotClipStatus::Saved);

    let saved = fs::read_to_string(&path).unwrap();
    assert!(saved.contains(r#""schema":"knot.clip.insert/v2""#));
    assert!(saved.contains(&chirograph::Sha256NamedInformation::of(&bytes).to_string()));
    assert!(saved.contains(&format!("blake3:{digest}")));
    assert!(!saved.contains("urn:blake3:"));
    assert!(!saved.contains("<article>"));

    drop(handle);
    drop(engine);
    std::thread::sleep(POLL_INTERVAL * 2);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let blobs = transport::BlobStore::open(&evidence).await.unwrap();
        let hash = transport::BlobHash::from_bytes(*blake3::hash(&bytes).as_bytes());
        assert_eq!(
            blobs.get_bytes(hash).await.unwrap().as_ref(),
            bytes.as_slice()
        );
        blobs.shutdown().await.unwrap();
    });
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(evidence).unwrap();
}

#[test]
#[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
fn resolve_and_run_cross_the_consented_authoring_path() {
    let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
        .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
    let root =
        std::env::temp_dir().join(format!("turnstone-knot-effects-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let included = root.join("included.md");
    fs::write(&included, "## Included\n\nFetched text.\n").unwrap();
    let path = root.join("field.knot");
    let authored = format!(
        "# Field\n\n```include {}\nFallback.\n```\n\n```rhai eval\n40 + 2\n```\n",
        file_address(&included)
    );
    fs::write(&path, &authored).unwrap();
    let address = file_address(&path);

    {
        let engine = KnotAuthoringEngine::connect_directory_effects(
            program.clone(),
            &root,
            4096,
            "ask",
            "ask",
        )
        .unwrap();
        let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
        let mut session = engine.spawn(&request).unwrap();
        let session = session
            .as_any()
            .downcast_mut::<KnotDocumentSession>()
            .unwrap();
        assert!(session.runner.state().resolve_available);
        assert!(session.runner.state().run_available);

        session.invoke_effect(KnotEffectKind::Resolve);
        let resolved = wait_for_derived(session, "resolved 1");
        assert!(resolved.source.contains("Included"));
        assert!(resolved.source.contains("rhai eval"));
        assert_eq!(fs::read_to_string(&path).unwrap(), authored);

        session.invoke_effect(KnotEffectKind::Run);
        let ran = wait_for_derived(session, "ran 1");
        assert!(ran.source.contains("Included"));
        assert!(ran.source.contains("42"));
        assert_eq!(fs::read_to_string(&path).unwrap(), authored);

        focus_editor(session);
        type_text(session, "local edit");
        assert!(
            session.runner.state().derived.is_none(),
            "an edit must drop a derived preview tied to older source"
        );
    }

    {
        let engine =
            KnotAuthoringEngine::connect_directory_effects(program, &root, 4096, "auto", "auto")
                .unwrap();
        let request = SessionSpawnRequest::new(&address).with_viewport(900, 600);
        let mut session = engine.spawn(&request).unwrap();
        let session = session
            .as_any()
            .downcast_mut::<KnotDocumentSession>()
            .unwrap();
        let ran = wait_for_derived(session, "ran 1");
        assert!(ran.source.contains("Included"));
        assert!(ran.source.contains("42"));
        assert_eq!(fs::read_to_string(&path).unwrap(), authored);
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "receipt: set KNOT_ENDPOINT_TEST_BIN to a built Mere knot_endpoint"]
fn received_communal_auto_run_waits_for_explicit_confirmation() {
    let program = std::env::var_os("KNOT_ENDPOINT_TEST_BIN")
        .expect("KNOT_ENDPOINT_TEST_BIN must name the real Knot endpoint");
    let root =
        std::env::temp_dir().join(format!("turnstone-knot-commons-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let authored = "\
# Received calculation

```rhai eval
40 + 2
```
";

    {
        let engine =
            KnotAuthoringEngine::connect_communal_fixture_effects(program, &root, 4096, "auto")
                .unwrap();
        let request = SessionSpawnRequest::new("knot://vault/received").with_viewport(900, 600);
        let mut session = engine.spawn(&request).unwrap();
        let session = session
            .as_any()
            .downcast_mut::<KnotDocumentSession>()
            .unwrap();
        assert!(session.runner.state().run_available);
        assert_eq!(session.runner.state().editor.source(), authored);

        wait_for_status(session, "rejected");
        assert_eq!(
            session.runner.state().detail,
            "received Commons documents require explicit effect confirmation"
        );
        assert!(session.runner.state().derived.is_none());

        session.invoke_effect(KnotEffectKind::Run);
        let ran = wait_for_derived(session, "ran 1");
        assert!(ran.source.contains("42"));
        assert_eq!(session.runner.state().editor.source(), authored);
    }

    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !tree_contains(&root, authored.as_bytes()),
        "received source must remain sealed in the process fixture root"
    );
    fs::remove_dir_all(root).unwrap();
}
