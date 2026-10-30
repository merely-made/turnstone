// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! App-spine tests: every one drives `Action` -> `update` -> `Effect` and
//! asserts on app truth, so they read as the spine's own specification.

use super::*;

// Future-supplier controls: apply only after the resource-content API is
// integrated as an exact, coordinated supplier set. These are product read
// and recycle-label checks, not an attributed recovery receipt.
fn resource_content_fixture() -> (App, uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    use mere::kernel::graph::apply::add_node;
    use mere::kernel::graph::resource_tags::{TagConcept, tag_concept_iri};
    use mere::kernel::graph::{Author, Graph};

    let mut graph = Graph::new();
    let first = uuid::Uuid::from_u128(101);
    let second = uuid::Uuid::from_u128(102);
    let a = add_node(
        &mut graph,
        Some(first),
        "https://EXAMPLE.test/page#one".into(),
        Default::default(),
    );
    let b = add_node(
        &mut graph,
        Some(second),
        "https://example.test/page#two".into(),
        Default::default(),
    );
    let resource = graph
        .shown_resource_id(a)
        .expect("live add records resource");
    assert_eq!(graph.shown_resource_id(b), Some(resource));
    graph.write_as(Author::person("content-fixture"), |graph| {
        let owner_iri = graph.write_author().asserter_iri();
        for label in ["Paper", "paper"] {
            assert!(
                graph
                    .tag_resource_with_concept(
                        resource,
                        &tag_concept_iri(&owner_iri, label),
                        TagConcept {
                            owner_iri: owner_iri.clone(),
                            label: label.into(),
                        },
                    )
                    .unwrap()
            );
        }
    });
    // A raw Surface read is a deliberately broken consumer control: none of
    // these labels was copied into either Surface's legacy tag column.
    assert!(graph.get_node(a).unwrap().tags.is_empty());
    assert!(graph.get_node(b).unwrap().tags.is_empty());
    let mut app = App::test_stub();
    app.graph_runtimes.set_graph(graph);
    app.bind_behavior_journal().expect("test baseline");
    let pane = app.default_graph_pane();
    assert!(app.graph_pane_select_member(pane, first));
    (app, first, second, resource)
}

#[cfg(any(feature = "piccolo", feature = "wasm"))]
fn participant_author(resident: &crate::denizen::Resident) -> mere::kernel::graph::Author {
    mere::kernel::graph::Author::script(
        resident.subject.to_hex(),
        blake3::Hash::from(resident.binding.revision.0)
            .to_hex()
            .to_string(),
    )
    .via("turnstone")
}

fn requested_source_capture(
    app: &mut App,
    node: uuid::Uuid,
) -> crate::source_capture::CaptureRequest {
    if app.source_capture_target(node).is_err() {
        let url = app
            .graph_runtimes
            .graph()
            .get_node_by_id(node)
            .unwrap()
            .1
            .url()
            .to_owned();
        let body = "<html><main>source fixture</main></html>";
        app.content.note_fetched(
            node,
            url.clone(),
            crate::content::FetchedDocument {
                bytes: body.as_bytes().to_vec(),
                body: body.into(),
                content_type: Some("text/html".into()),
                effective_url: Some(url),
                acquired_at_ms: 1,
            },
            body.len(),
        );
    }
    let target = app
        .source_capture_target(node)
        .expect("complete fixture source");
    app.update(Action::CaptureSourceDocument { target })
        .into_iter()
        .find_map(|effect| match effect {
            Effect::CaptureSourceDocument { request, .. } => Some(request),
            _ => None,
        })
        .expect("current target produces a correlated source request")
}

fn inline_artifact(bytes: &[u8]) -> crate::place::invite::ArtifactRefV1 {
    crate::place::invite::ArtifactRefV1::Inline {
        media_type: "application/vnd.mere.place-artifact".into(),
        digest: proofs::Digest::blake3(bytes)
            .bytes
            .as_slice()
            .try_into()
            .unwrap(),
        bytes: bytes.to_vec(),
    }
}

fn micron_ready_to_send(source: &str) -> App {
    let mut app = App::test_stub();
    let address = "7fc8950ec4e50695be76eddc8e539ee8:/page/index.mu";
    app.update(Action::OpenAddress(address.into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.content.note_fetched(
        node,
        address.into(),
        crate::content::FetchedDocument {
            bytes: source.as_bytes().to_vec(),
            content_type: None,
            body: source.into(),
            effective_url: Some(address.into()),
            acquired_at_ms: 1,
        },
        source.len(),
    );
    app.update(Action::ComposeFocusedMicronForm);
    app
}

fn open_place(
    local: [u8; 32],
    members: Vec<crate::place::PlaceMember>,
) -> crate::place::PlaceState {
    crate::place::PlaceState::Offline {
        binding: crate::place::PlaceBindingV1::new(
            crate::place::PlaceId([0xe1; 32]),
            crate::place::SharedContainerId([0xe2; 32]),
            crate::place::ChatSpaceId([0xe3; 32]),
            "general",
        )
        .unwrap(),
        generation: 5,
        snapshot: crate::place::OfflinePlaceSnapshot {
            personae_root: local,
            members,
            ..Default::default()
        },
    }
}

#[path = "tests/a_behavior_wakes_on_an_app_event_without_cases.rs"]
#[cfg(feature = "piccolo")]
mod a_behavior_wakes_on_an_app_event_without_cases;
#[path = "tests/a_rerooted_profile_refuses_automatic_certificate_reissue_cases.rs"]
mod a_rerooted_profile_refuses_automatic_certificate_reissue_cases;
#[path = "tests/a_running_behavior_does_not_re_enter_the_cases.rs"]
#[cfg(feature = "piccolo")]
mod a_running_behavior_does_not_re_enter_the_cases;
#[path = "tests/close_session_trashes_and_recover_restores_identity_cases.rs"]
mod close_session_trashes_and_recover_restores_identity_cases;
#[path = "tests/graph_navigation_read_adds_a_tab_and_keeps_cases.rs"]
mod graph_navigation_read_adds_a_tab_and_keeps_cases;
#[path = "tests/keep_action_is_target_bound_idempotent_and_observable_cases.rs"]
mod keep_action_is_target_bound_idempotent_and_observable_cases;
#[path = "tests/malformed_place_binding_is_visible_without_hiding_cached_cases.rs"]
mod malformed_place_binding_is_visible_without_hiding_cached_cases;
#[path = "tests/micron_form_refuses_submission_after_source_address_changes_cases.rs"]
mod micron_form_refuses_submission_after_source_address_changes_cases;
#[path = "tests/place_artifacts_are_written_where_the_person_asked_cases.rs"]
mod place_artifacts_are_written_where_the_person_asked_cases;
#[path = "tests/place_collection_selection_lowers_the_exact_version_and_cases.rs"]
mod place_collection_selection_lowers_the_exact_version_and_cases;
#[path = "tests/place_revoke_rows_are_a_founders_one_per_cases.rs"]
mod place_revoke_rows_are_a_founders_one_per_cases;
#[path = "tests/recall_never_outranks_the_typed_address_cases.rs"]
mod recall_never_outranks_the_typed_address_cases;
#[path = "tests/resource_content_inspector_follows_shared_resource_and_navigation_cases.rs"]
mod resource_content_inspector_follows_shared_resource_and_navigation_cases;
#[path = "tests/sensitive_smolweb_input_is_masked_and_never_enters_cases.rs"]
mod sensitive_smolweb_input_is_masked_and_never_enters_cases;
#[path = "tests/toolbar_tool_toggle_retains_identity_state_and_restores_cases.rs"]
mod toolbar_tool_toggle_retains_identity_state_and_restores_cases;
