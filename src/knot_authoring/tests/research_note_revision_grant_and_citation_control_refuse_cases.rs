// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn research_note_revision_grant_and_citation_control_refuse_stale_edits() {
    let temp = tempfile::tempdir().unwrap();
    let notes = temp.path().join("notes");
    fs::create_dir(&notes).unwrap();
    let path = notes.join("field.knot");
    let (source, target) = crate::knot_saved_citations::tests::fixture();
    fs::write(&path, &source).unwrap();
    let (hub, _) = KnotHub::host(
        HostedKnot::Directory { root: notes },
        None,
        Some(HostedEvidence {
            root: temp.path().join("evidence"),
            max_artifact_bytes: 65536,
        }),
        65536,
        Arc::new(|| {}),
    )
    .unwrap();
    let address = file_address(&path);
    let mut note = KnotDocumentSession::new(
        hub.clone(),
        hub.open(&address).unwrap(),
        (900, 600),
        false,
        false,
        false,
    );
    let offered = note.research_clip_handle().unwrap();
    let reader = crate::saved_sources::address(&target).unwrap();
    let expected_source = blake3::hash(source.as_bytes());
    note.runner.update(|state| {
        state.requests.push(AuthoringRequest::OpenSavedCitation {
            address: reader.clone(),
            expected_source,
        })
    });
    assert_eq!(note.run_authoring_requests(), Some(reader.clone()));
    focus_editor(&mut note);
    type_text(&mut note, "Local unsaved commentary.");
    assert!(note.research_clip_handle().is_err());
    note.runner.update(|state| {
        state.requests.push(AuthoringRequest::OpenSavedCitation {
            address: reader,
            expected_source,
        })
    });
    assert_eq!(note.run_authoring_requests(), None);
    assert!(note.runner.state().detail.contains("changed"));
    // An offer already queued at the endpoint cannot overwrite a later save.
    fs::write(&path, format!("{source}\nRemote saved revision.\n")).unwrap();
    let clip = inker::DocumentClip {
        source_url: "https://gone.test".into(),
        title: None,
        text: "Stale clip must not appear".into(),
        selector: None,
        links: vec![],
        artifacts: vec![inker::DocumentClipArtifact {
            role: inker::DocumentClipArtifactRole::ObservedRepresentation,
            media_type: "text/plain".into(),
            canonical_uri: "https://gone.test".into(),
            bytes: b"Stale clip must not appear".to_vec(),
        }],
    };
    offered.insert(clip).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while matches!(offered.status(), KnotClipStatus::Sending) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        matches!(
            offered.status(),
            KnotClipStatus::Rejected(_) | KnotClipStatus::Stale
        ),
        "{:?}",
        offered.status()
    );
    note.drain_events();
    assert!(
        note.runner
            .state()
            .editor
            .source()
            .contains("Local unsaved commentary.")
    );
    assert!(
        !fs::read_to_string(path)
            .unwrap()
            .contains("Stale clip must not appear")
    );
}

#[test]
fn semantic_clip_body_omits_document_frontmatter() {
    let fragment = fleece::clip::fragment_from_text(
        "https://example.test/post",
        Some("Interesting bit".into()),
        "A useful paragraph.",
        Some("main > article".into()),
        vec!["https://example.test/ref".into()],
    );
    let body = clip_knot_body(&fragment);
    assert!(body.contains("# Interesting bit"));
    assert!(body.contains("A useful paragraph."));
    assert!(body.contains("https://example.test/ref"));
    assert!(!body.starts_with("---\n"));
    assert!(!body.contains("source: https://example.test/post"));
}

#[test]
fn knot_clip_preserves_micron_local_jump_as_label_text() {
    let mut registry = inker::EngineRegistry::new();
    for engine in nematic::engines() {
        registry.register(engine);
    }
    let mut policy = inker::EngineRoutePolicy::default();
    policy.fallback.engine_id = nematic::micron::ENGINE_MICRON.into();
    let fragment = fleece::clip::fragment_from_body(
        "0123456789abcdef0123456789abcdef:/page/index.mu",
        None,
        Some("text/x-micron"),
        "`[Reference`:/page/ref.mu] `[Setup notes`#setup]\n>Setup\n",
        &registry,
        &policy,
    );
    let body = clip_knot_body(&fragment);
    assert!(body.contains("Setup notes"));
    assert!(!body.contains("#setup"));
}

/// A non-founder process inherits every name its parent exported, blank
/// ones included: `TURNSTONE_KNOT_ROOT=` must read as "no vault of my own",
/// not as a path at the current directory.
/// The neutral session keys reach Knot's editor as the Cambium events its
/// own key path took, modifiers included (SC controller-level input).
#[test]
fn neutral_session_keys_become_cambium_key_events() {
    let modifiers = inker::SessionModifiers {
        control: true,
        ..Default::default()
    };
    let save = super::super::cambium_key(inker::SessionKey::Character("s".to_owned()), modifiers)
        .expect("a character maps");
    assert!(matches!(&save.key, cambium::Key::Character(text) if text == "s"));
    assert!(save.mods.ctrl && !save.mods.shift);
    assert!(matches!(
        super::super::cambium_key(inker::SessionKey::Tab, Default::default())
            .map(|event| event.key),
        Some(cambium::Key::Named(cambium::NamedKey::Tab))
    ));
    assert!(
        super::super::cambium_key(inker::SessionKey::Unidentified, Default::default()).is_none()
    );
}

#[test]
fn a_blank_path_setting_reads_as_unset() {
    let name = "TURNSTONE_KNOT_ROOT_ENV_PATH_PROBE";
    // SAFETY: a name used by this test alone, set and read on one thread.
    unsafe {
        std::env::remove_var(name);
    }
    assert_eq!(env_path(name), None, "an absent name is unset");
    for blank in ["", " ", "\t  \n"] {
        unsafe {
            std::env::set_var(name, blank);
        }
        assert_eq!(
            env_path(name),
            None,
            "a blank value is unset, not a path at the current directory"
        );
    }
    unsafe {
        std::env::set_var(name, " C:/vault ");
    }
    assert_eq!(
        env_path(name),
        Some(PathBuf::from(" C:/vault ")),
        "a value with content is passed through verbatim, spaces and all"
    );
    unsafe {
        std::env::remove_var(name);
    }
}

#[test]
fn synced_knot_head_becomes_public_scenograph_revision_without_save_token() {
    let head = [0x5a; 32];
    let editable = editable_projection_fixture(
        "knot://vault/field-note",
        Some(head),
        vec![0xde, 0xad, 0xbe, 0xef],
    );

    let input = knot_projection_input_binding(&editable);
    assert_eq!(input.source.authority, KNOT_PROJECTION_AUTHORITY);
    assert_eq!(input.source.domain, KNOT_PROJECTION_DOMAIN);
    assert_eq!(input.source.resource, "knot://vault/field-note");
    assert_eq!(
        input.expects_generation,
        Some(PublicSourceRevision::new(knot::hex32(&head)))
    );
    assert_eq!(input.revision_evidence, RevisionEvidence::PublicGeneration);
    assert!(editable.public_revision.is_some());

    let definition = knot_projection_recipe(input)
        .bind("document", None)
        .expect("a public source generation binds durably");
    let json = String::from_utf8(definition.to_json_bytes().unwrap()).unwrap();
    assert!(json.contains(&knot::hex32(&head)));
    assert!(!json.contains("base_token"));
    assert!(!json.contains("deadbeef"));
}

#[test]
fn unversioned_knot_source_requires_a_runtime_witness_without_durable_token() {
    let editable = editable_projection_fixture(
        "file:///C:/notes/field.knot",
        None,
        vec![0xde, 0xad, 0xbe, 0xef],
    );
    let input = knot_projection_input_binding(&editable);
    assert_eq!(input.expects_generation, None);
    assert_eq!(input.revision_evidence, RevisionEvidence::RuntimeVerified);
    let runtime_source =
        knot_runtime_source_binding(input.source.clone(), editable.base_token.clone());

    let binding = knot_projection_recipe(input)
        .bind_runtime("document", runtime_source, None)
        .expect("runtime-verified source binds only in this process");
    assert!(binding.with_witness(|witness| witness.matches_save_token(&editable.base_token)));
    let recorded = binding.into_recorded_definition();
    assert_eq!(recorded.provenance.source_revision, None);
    assert_eq!(
        recorded.provenance.revision_evidence,
        RevisionEvidence::RuntimeVerified
    );
    let json = String::from_utf8(recorded.to_json_bytes().unwrap()).unwrap();
    assert!(json.contains("\"revision_evidence\":\"runtime_verified\""));
    assert!(!json.contains("source_revision"));
    assert!(!json.contains("base_token"));
    assert!(!json.contains("deadbeef"));
}

#[test]
fn sealed_cache_age_uses_readable_units() {
    assert_eq!(format_cache_age(999), "0s");
    assert_eq!(format_cache_age(59_999), "59s");
    assert_eq!(format_cache_age(60_000), "1m");
    assert_eq!(format_cache_age(3_600_000), "1h");
    assert_eq!(format_cache_age(86_400_000), "1d");
}

#[test]
fn sealed_cache_attribution_is_visible_in_the_derived_preview() {
    let derived = DerivedTextV1 {
        source: "# Cached".into(),
        summary: "resolved 1; denied 0; failed 0".into(),
        cache: Some(chirograph::DerivedCacheInfoV1 {
            effect: "resolve".into(),
            sources: vec!["https://example.test/note".into()],
            provider_version: "fixture/v1".into(),
            policy_fingerprint: "policy".into(),
            fetched_at_unix_ms: 1_000,
            source_revision: 1,
        }),
    };

    assert_eq!(
        derived_status_at(&derived, 61_000),
        "resolved 1; denied 0; failed 0\nFetched: 1m ago; 1 source"
    );
}

#[test]
fn projected_action_is_probe_drivable_by_instance_identity() {
    let dom: DomHandle = Rc::new(std::cell::RefCell::new(ScriptedDom::new()));
    let _runner = AuthoringRunner::new(
        dom.clone(),
        authoring_view as fn(&AuthoringState) -> AuthoringView,
        AuthoringState {
            editor: KnotEditor::scratch("knot:test", "# Test"),
            projection_target: InstanceId(17),
            last_intent: Some(ProjectionIntentReceipt {
                target: InstanceId(17),
                intent: KNOT_TRANSCLUSION_RESOLVE_INTENT.into(),
                result: IntentResult::Accepted,
            }),
            status: AuthoringStatus::Current,
            detail: String::new(),
            requests: Vec::new(),
            resolve_available: true,
            run_available: true,
            derived: None,
            width: 900,
            height: 600,
        },
    );
    let dom = dom.borrow();
    let surfaces = [taproot::ProbeSurface {
        name: "knot-authoring",
        dom: &dom,
        rect: [0.0, 0.0, 900.0, 600.0],
        sheet: KNOT_SHEET,
    }];

    let hit = taproot::resolve_estimated_fixture(
        &surfaces,
        &taproot::Selector::class("knot-resolve").with_attr("data-projection-instance", "17"),
    );
    assert!(
        hit.is_some(),
        "the probe must resolve the action by InstanceId"
    );
    assert!(taproot::text_present(
        &surfaces,
        "Projection intent 17: knot.transclusion.resolve accepted"
    ));
}

#[test]
fn marked_div_text_control_is_discovered_without_a_native_tag() {
    type FixtureView = Box<dyn AnyView<(), (), GenetCtx, GenetElement>>;
    fn marked_fixture(_: &()) -> FixtureView {
        Box::new(
            el::<_, (), ()>("div", "draft")
                .attr("role", "textbox")
                .attr("data-cambium-text-value", "draft"),
        )
    }

    let dom: DomHandle = Rc::new(std::cell::RefCell::new(ScriptedDom::new()));
    let _runner = GenetAppRunner::new(dom.clone(), marked_fixture as fn(&()) -> FixtureView, ());
    let dom = dom.borrow();
    let field = find_text_control(&dom, dom.document())
        .expect("the marked DIV is recognized as the editor field");
    assert_eq!(
        dom.element_name(field).map(|name| name.local.as_ref()),
        Some("div")
    );
    assert!(
        dom.attribute(
            field,
            &layout_dom_api::Namespace::from(""),
            &layout_dom_api::LocalName::from("aria-label"),
        )
        .is_none(),
        "the unnamed Knot field stays unnamed"
    );
}

#[test]
fn knot_addresses_are_selected_without_claiming_other_files() {
    assert!(is_knot_address("file:///C:/notes/one.djot"));
    assert!(is_knot_address("file:///C:/notes/one.knot"));
    assert!(is_knot_address("KNOT:vault/document"));
    assert!(is_knot_address("file:///tmp/one.DJOT?mode=edit"));
    assert!(is_knot_address("file:///tmp/one.KNOT#heading"));
    assert!(!is_knot_address("file:///tmp/one.md"));
    assert!(!is_knot_address("https://example.test/"));
}

#[test]
fn a_place_held_address_names_its_holder_and_stays_a_knot_address() {
    let root = [0xab; 32];
    let address = place_held_address(&root, "notes/field.knot");
    assert_eq!(
        address,
        format!("knot://{}/notes/field.knot", "ab".repeat(32))
    );
    assert!(
        is_knot_address(&address),
        "the ordinary open route must still recognize it"
    );
    assert_eq!(
        parse_place_held_address(&address),
        Some((root, "notes/field.knot".to_string()))
    );
}

/// T5c's rootless half. A member with no vault of its own still has to be
/// able to visit someone else's document: the engine registers either way,
/// so a place-held address reaches the visiting path (and fails there,
/// about the visit) instead of falling off the content registry into the
/// external-protocol route. Only a LOCAL address is refused, by name.
#[test]
fn a_member_with_no_vault_visits_but_holds_nothing() {
    let engine = KnotAuthoringEngine::visit_only(KnotVisits::default(), None);

    let local = "file:///C:/vault/field.knot";
    assert!(is_knot_address(local));
    let refusal = engine
        .spawn(&SessionSpawnRequest::new(local))
        .err()
        .expect("a local address has no vault to open it");
    assert!(
        matches!(&refusal, SessionError::SpawnFailed(reason)
                if reason.contains("no Knot vault is configured")),
        "a local address is refused by the missing vault, not the route: {refusal:?}"
    );

    // The place-held address takes the VISITING branch. No visit is open in
    // this test, so it fails there — and that failure, naming the visit, is
    // the proof it never reached the local-vault branch.
    let held = place_held_address(&[0x5b; 32], "field.knot");
    let visiting = engine
        .spawn(&SessionSpawnRequest::new(held.clone()))
        .err()
        .expect("no visit is open yet");
    assert!(
        matches!(&visiting, SessionError::SpawnFailed(reason)
                if reason.contains("no visit is open to the mere holding")),
        "a place-held address routes to the visiting path: {visiting:?}"
    );
    assert!(
        !format!("{visiting:?}").contains("no Knot vault is configured"),
        "visiting must not need a vault of one's own"
    );

    // And the engine claims the address, which is what keeps the shell from
    // handing it to host.external-protocol.
    assert_eq!(engine.engine_id(), ENGINE_ID);
    assert!(matches!(
        engine.spawn(&SessionSpawnRequest::new("https://example.test/page")),
        Err(SessionError::Unsupported(_))
    ));
}

/// A visit-only engine has nowhere to put a clip, whatever the environment
/// names as the target.
#[test]
fn a_visit_only_engine_offers_no_clip_target() {
    let engine = KnotAuthoringEngine::visit_only(KnotVisits::default(), Some("field.knot".into()));
    assert!(engine.clip_handle().is_none());
}

#[test]
fn only_a_personae_root_is_read_as_a_holder() {
    // A local vault, a short host, a non-hex host, and a document with no
    // path are all Knot addresses and none of them names a holder.
    for address in [
        "knot://vault/field",
        "knot://abcd/field.knot",
        &format!("knot://{}/field.knot", "zz".repeat(32)),
        &format!("knot://{}", "ab".repeat(32)),
        &format!("knot://{}/", "ab".repeat(32)),
        "file:///C:/vault/field.knot",
    ] {
        assert_eq!(
            parse_place_held_address(address),
            None,
            "{address} must not be read as place-held"
        );
    }
}
