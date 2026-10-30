// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use cambium::{CompositionEvent, Modifiers};
use genet_scripted_dom::NodeId;
use layout_dom_api::{LayoutDom, NodeKind};

use super::*;

fn editable_projection_fixture(
    address: &str,
    public_revision: Option<[u8; 32]>,
    base_token: Vec<u8>,
) -> EditableTextV1 {
    EditableTextV1 {
        address: address.to_owned(),
        media_type: "text/vnd.knot".to_owned(),
        encoding: chirograph::TextEncoding::Utf8,
        source: "# Field\n".to_owned(),
        base_token,
        public_revision,
        derived: None,
    }
}

fn knot_projection_recipe(
    binding: ProjectionInputBinding,
) -> scenograph::AuthoredProjectionDefinition {
    use scenograph::{
        Appearance, Arrangement, AuthoredProjectionDefinition, Channel, Encoding, Interaction,
        PROJECTION_DEFINITION_VERSION, Provenance, Reading,
    };

    AuthoredProjectionDefinition {
        version: PROJECTION_DEFINITION_VERSION,
        id: "turnstone.knot.outline".to_owned(),
        label: "Knot outline".to_owned(),
        sources: BTreeMap::from([("document".to_owned(), binding)]),
        reading: Reading {
            kind: "knot.outline".to_owned(),
            key: "source_range".to_owned(),
            value: Some("heading".to_owned()),
        },
        encoding: Encoding {
            x: Channel::Field("source_range.start".to_owned()),
            y: Channel::Field("depth".to_owned()),
            color: None,
            label: Some(Channel::Field("heading".to_owned())),
        },
        arrangement: Arrangement::default(),
        dynamics: None,
        interaction: Interaction::default(),
        appearance: Appearance {
            realization: "turnstone.knot-outline".to_owned(),
            title: "Knot outline".to_owned(),
            theme: "turnstone".to_owned(),
        },
        provenance: Provenance {
            author: "turnstone-test".to_owned(),
            source_revision: Some(PublicSourceRevision::from("turnstone.knot.outline/v1")),
            revision_evidence: RevisionEvidence::PublicGeneration,
            note: "fixture".to_owned(),
        },
    }
}

fn file_address(path: &Path) -> String {
    let path = fs::canonicalize(path).expect("test document should canonicalize");
    #[cfg(windows)]
    {
        let path = path.to_string_lossy();
        let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
        format!("file:///{}", path.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        format!("file://{}", path.to_string_lossy())
    }
}

fn find_text_control(dom: &ScriptedDom, node: NodeId) -> Option<NodeId> {
    let tag = dom.element_name(node).map(|element| element.local.as_ref());
    let native = matches!(tag, Some("input" | "textarea"));
    let marked = dom.attribute(
        node,
        &layout_dom_api::Namespace::from(""),
        &layout_dom_api::LocalName::from("role"),
    ) == Some("textbox")
        && dom
            .attribute(
                node,
                &layout_dom_api::Namespace::from(""),
                &layout_dom_api::LocalName::from("data-cambium-text-value"),
            )
            .is_some();
    if native || marked {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| find_text_control(dom, child))
}

fn focus_editor(session: &mut KnotDocumentSession) {
    let textarea = {
        let dom = session.dom.borrow();
        let field = find_text_control(&dom, session.runner.root())
            .expect("authoring view should contain a text control");
        assert!(
            dom.attribute(
                field,
                &layout_dom_api::Namespace::from(""),
                &layout_dom_api::LocalName::from("aria-label"),
            )
            .is_none(),
            "the Knot editor remains unnamed"
        );
        field
    };
    session
        .runner
        .dispatch_click(textarea, PointerClick::at((1.0, 1.0)));
    assert_eq!(session.runner.focus(), Some(textarea));
}

fn wait_for_status(session: &mut KnotDocumentSession, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = session.status();
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected status {expected}, found {actual}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_refresh(session: &mut KnotDocumentSession, previous: u64) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.revision_refreshes == previous {
        session.drain_events();
        assert!(
            Instant::now() < deadline,
            "endpoint revision bell did not reach the dirty editor"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_derived(session: &mut KnotDocumentSession, summary_prefix: &str) -> DerivedTextV1 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        session.drain_events();
        if let Some(derived) = session.runner.state().derived.as_ref()
            && derived.summary.starts_with(summary_prefix)
        {
            return derived.clone();
        }
        assert!(
            Instant::now() < deadline,
            "expected derived result starting with {summary_prefix}, status {}",
            session.status()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn type_text(session: &mut KnotDocumentSession, text: &str) {
    assert!(session.dispatch_key(KeyEvent::new(Key::Character(text.into()))));
}

fn save_shortcut(session: &mut KnotDocumentSession) {
    assert!(session.dispatch_key(KeyEvent::with_mods(
        Key::Character("s".into()),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    )));
}

fn tree_contains(root: &Path, needle: &[u8]) -> bool {
    fs::read_dir(root).unwrap().flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            tree_contains(&path, needle)
        } else {
            fs::read(path)
                .ok()
                .is_some_and(|bytes| bytes.windows(needle.len()).any(|part| part == needle))
        }
    })
}

fn wait_for_clip_status(handle: &KnotClipHandle, expected: KnotClipStatus) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let actual = handle.status();
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected clip status {expected:?}, found {actual:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[path = "tests/research_note_revision_grant_and_citation_control_refuse_cases.rs"]
mod research_note_revision_grant_and_citation_control_refuse_cases;
#[path = "tests/saved_selection_from_embedded_pool_pointer_gesture_cases.rs"]
mod saved_selection_from_embedded_pool_pointer_gesture_cases;
#[path = "tests/sharing_rewrites_a_locally_held_document_to_its_cases.rs"]
mod sharing_rewrites_a_locally_held_document_to_its_cases;
#[path = "tests/note_scroll_cases.rs"]
mod note_scroll_cases;
