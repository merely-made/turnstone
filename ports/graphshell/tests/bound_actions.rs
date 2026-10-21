// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#![cfg(any(feature = "native", feature = "web"))]

use std::collections::BTreeMap;

use chirograph::{
    ActionFormChoiceV1, ActionFormFieldV1, ActionFormV1, AdvertisedAction, BoundsRelationship,
    CachePolicy, CacheRetention, CapabilityProfile, CarrierRequestBody, CarrierResponseBody,
    ContentHash, EndpointDescriptor, IntentEffect, IntentReference, IntentResult,
    PresentationBinding, PresentationCapability, PresentationCodec, PresentationKey,
    PresentationManifest, PresentationOffer, PresentationSemantics, ProjectionOffer,
    ProjectionRequest, ProjectionSession as OwnerSession, ProjectionSnapshot, ProtocolVersion,
    Revision, SceneEpoch, SemanticRole,
};
use graphshell::bound_actions::BoundOwnerActions;
use graphshell::local_projection::ProjectionSession;
use graphshell::projection_compile::ProjectionDataset;
use graphshell_client::{Outcome, Progress, SessionCore};
use sceno::{Footprint, InstanceId, ProjectedItem, Representation, Scene, Size2, Transform2};
use scenograph::RuntimeSourceBinding;
use scenotime::SceneSnapshot;

const SOURCE: &str = include_str!("../web/fixtures/woodshed-stage.json");
const INTENT: &str = "fixture:choose";

// A scripted endpoint disclosure, not a live Woodshed action implementation.
fn fixture() -> (
    ProjectionSession,
    SessionCore,
    ProjectionSnapshot,
    BTreeMap<String, InstanceId>,
) {
    let dataset: ProjectionDataset = serde_json::from_str(SOURCE).unwrap();
    // The fixture already has two distinct occurrences of the same chord.
    assert_eq!(dataset.occurrences[1].source, dataset.occurrences[0].source);
    let view = ProjectionSession::load(&serde_json::to_string(&dataset).unwrap(), None).unwrap();
    let session = OwnerSession("fixture:owner".into());
    let mut scene = Scene::new();
    let mut presentation = PresentationManifest::default();
    let mut targets = BTreeMap::new();
    // Owner IDs deliberately differ from the locally compiled IDs.
    for (index, occurrence) in dataset
        .occurrences
        .iter()
        .cycle()
        .skip(1)
        .take(dataset.occurrences.len())
        .enumerate()
    {
        let target = InstanceId(index as u32);
        targets.insert(occurrence.occurrence_id.clone(), target);
        let source = scene.intern_source(occurrence.source.clone());
        scene.items.push(ProjectedItem {
            source,
            space: Scene::WORLD,
            transform: Transform2::translation(index as f32 * 40.0, 0.0),
            footprint: Footprint::Rect {
                size: Size2::new(30.0, 20.0),
            },
            representation: Representation::Glyph,
            layer: 0,
            visible: true,
            hit: None,
            channels: vec![],
        });
        let key = PresentationKey(format!("owner:{index}"));
        presentation.bindings.push(PresentationBinding {
            instance: target,
            key: key.clone(),
        });
        presentation.offers.insert(
            key,
            vec![PresentationOffer {
                codec: PresentationCodec::NativeGlyphV1,
                resource: ContentHash::of(b"unfetched"),
                byte_size: 9,
                requires: PresentationCapability::NativeGlyph,
                semantics: PresentationSemantics {
                    label: occurrence.occurrence_id.clone(),
                    role: SemanticRole::Graphic,
                    bounds: BoundsRelationship::FillFootprint,
                    actions: vec![AdvertisedAction {
                        intent: IntentReference(INTENT.into()),
                        label: format!("Choose on {}", occurrence.occurrence_id),
                        explanation: "Scripted owner contract".into(),
                        payload_schema: "fixture.choose/v1".into(),
                        input_form: Some(ActionFormV1::new("fixture.choose/v1").with_field(
                            ActionFormFieldV1::choice(
                                "choice",
                                "Choice",
                                [ActionFormChoiceV1::new(
                                    &occurrence.occurrence_id,
                                    "This occurrence",
                                )],
                            ),
                        )),
                        effect: IntentEffect::DomainTruth,
                    }],
                },
            }],
        );
    }
    let snapshot = ProjectionSnapshot {
        version: ProtocolVersion::V1,
        session: session.clone(),
        scene: SceneSnapshot::from_dense(SceneEpoch(3), Revision(7), scene).unwrap(),
        presentation,
        cache_policy: CachePolicy {
            retention: CacheRetention::MemoryOnly,
            expires_at_ms: None,
            purge_on_revocation: true,
        },
    };
    let mut owner = SessionCore::from_descriptor(
        EndpointDescriptor {
            label: "Scripted owner".into(),
            projections: vec![ProjectionOffer {
                label: "Current material".into(),
                request: ProjectionRequest {
                    version: ProtocolVersion::V1,
                    session,
                    score: view.compiled().unwrap().score.clone(),
                },
            }],
        },
        CapabilityProfile::new([PresentationCapability::NativeGlyph]),
    );
    assert!(matches!(owner.mount(0).unwrap(), Progress::Ask(_)));
    assert!(matches!(
        owner
            .on_response(CarrierResponseBody::Snapshot(Box::new(snapshot.clone())))
            .unwrap(),
        Progress::Done(Outcome::Mounted(_))
    ));
    (view, owner, snapshot, targets)
}

#[test]
fn repeated_source_uses_exact_owner_occurrence_and_existing_form_and_wire() {
    let (mut view, mut owner, snapshot, targets) = fixture();
    view.select("card:2").unwrap();
    let expected = targets["card:2"];
    assert_ne!(view.compiled().unwrap().selected, Some(expected));
    let before = view.snapshot().unwrap();
    {
        let mut bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(view.dataset().source.clone(), targets),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        let actions = bound
            .actions(view.dataset(), view.compiled().unwrap())
            .unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "Choose on card:2");
        let (target, mut draft) = bound
            .open_action(view.dataset(), view.compiled().unwrap(), INTENT)
            .unwrap();
        assert!(draft.choose("choice", "card:1").is_err());
        draft.choose("choice", "card:2").unwrap();
        let progress = bound
            .submit_action(
                view.dataset(),
                view.compiled().unwrap(),
                &target,
                &mut draft,
            )
            .unwrap();
        let Some(CarrierRequestBody::Intent(wire)) = progress.request() else {
            panic!("intent request")
        };
        assert_eq!(wire.target, expected);
        assert_eq!(wire.session, snapshot.session);
        assert_eq!(wire.observed_epoch, SceneEpoch(3));
        assert_eq!(wire.observed_revision, Revision(7));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&wire.payload).unwrap()["choice"],
            "card:2"
        );
        assert!(
            bound
                .submit_action(
                    view.dataset(),
                    view.compiled().unwrap(),
                    &target,
                    &mut draft
                )
                .unwrap_err()
                .contains("awaiting")
        );
    }
    let outcome = owner
        .on_response(CarrierResponseBody::Intent(IntentResult::Accepted))
        .unwrap()
        .done()
        .unwrap();
    let Outcome::Intent(result) = outcome else {
        panic!("intent outcome")
    };
    assert_eq!(
        result.command_outcome(),
        None,
        "dispatch acceptance is not completion"
    );
    assert_eq!(
        view.snapshot().unwrap(),
        before,
        "actions do not author recipes"
    );
    let mut reopened = ProjectionSession::load(SOURCE, None).unwrap();
    reopened.reopen(&before).unwrap();
    assert_eq!(reopened.selected(), Some("card:2"));
    let saved = serde_json::to_string(&before).unwrap();
    assert!(!saved.contains("fixture:owner"));
    assert!(!saved.contains(INTENT));
}

#[test]
fn missing_mapping_wrong_source_and_changed_selection_do_not_guess_a_target() {
    let (mut view, mut owner, snapshot, mut targets) = fixture();
    view.select("card:2").unwrap();
    let removed = targets.remove("card:2").unwrap();
    {
        let bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(view.dataset().source.clone(), targets.clone()),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        assert!(
            bound
                .actions(view.dataset(), view.compiled().unwrap())
                .unwrap_err()
                .contains("exact action target")
        );
    }
    targets.insert("card:2".into(), removed);
    let mut source = view.dataset().source.clone();
    source.authority = "another owner".into();
    {
        let bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(source, targets.clone()),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        assert!(
            bound
                .actions(view.dataset(), view.compiled().unwrap())
                .unwrap_err()
                .contains("different source")
        );
    }
    {
        let mut bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(view.dataset().source.clone(), targets.clone()),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        let (target, mut draft) = bound
            .open_action(view.dataset(), view.compiled().unwrap(), INTENT)
            .unwrap();
        draft.choose("choice", "card:2").unwrap();
        view.select("card:1").unwrap();
        assert!(
            bound
                .submit_action(
                    view.dataset(),
                    view.compiled().unwrap(),
                    &target,
                    &mut draft
                )
                .unwrap_err()
                .contains("target changed")
        );
    }
    let wrong = view
        .dataset()
        .occurrences
        .last()
        .unwrap()
        .occurrence_id
        .clone();
    {
        let bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(view.dataset().source.clone(), targets.clone()),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        let mut mismatched = view.dataset().clone();
        mismatched.occurrences[0].source = mismatched.occurrences.last().unwrap().source.clone();
        assert!(
            bound
                .actions(&mismatched, view.compiled().unwrap())
                .unwrap_err()
                .contains("compiled selection")
        );
    }
    targets.insert("card:1".into(), targets[&wrong]);
    let bound = BoundOwnerActions::new(
        RuntimeSourceBinding::new(view.dataset().source.clone(), targets),
        &mut owner,
        &snapshot.session,
    )
    .unwrap();
    assert!(
        bound
            .actions(view.dataset(), view.compiled().unwrap())
            .unwrap_err()
            .contains("selected source")
    );
}

#[test]
fn cached_or_unsupported_owner_presentations_offer_no_live_actions() {
    let (mut view, mut owner, mut snapshot, targets) = fixture();
    view.select("card:2").unwrap();
    for offers in snapshot.presentation.offers.values_mut() {
        offers[0].requires = PresentationCapability::Image;
        offers[0].codec = PresentationCodec::ImageV1 {
            mime_type: "image/png".into(),
        };
    }
    owner.client_mut().apply_snapshot(snapshot.clone()).unwrap();
    {
        let bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(view.dataset().source.clone(), targets.clone()),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        assert!(
            bound
                .actions(view.dataset(), view.compiled().unwrap())
                .unwrap_err()
                .contains("supported presentation")
        );
    }
    owner.disconnect();
    let bound = BoundOwnerActions::new(
        RuntimeSourceBinding::new(view.dataset().source.clone(), targets),
        &mut owner,
        &snapshot.session,
    )
    .unwrap();
    assert!(
        bound
            .actions(view.dataset(), view.compiled().unwrap())
            .unwrap_err()
            .contains("not live")
    );
}

#[test]
fn changed_owner_offer_is_not_replayed_and_owner_refusal_and_stale_survive() {
    let (mut view, mut owner, mut snapshot, targets) = fixture();
    view.select("card:2").unwrap();
    let (target, mut draft) = {
        let bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(view.dataset().source.clone(), targets.clone()),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        bound
            .open_action(view.dataset(), view.compiled().unwrap(), INTENT)
            .unwrap()
    };
    draft.choose("choice", "card:2").unwrap();
    let original = snapshot.clone();
    for offers in snapshot.presentation.offers.values_mut() {
        offers[0].semantics.actions[0].payload_schema = "fixture.choose/v2".into();
    }
    owner.client_mut().apply_snapshot(snapshot.clone()).unwrap();
    assert!(
        owner
            .submit_action_draft(&target, &mut draft)
            .unwrap_err()
            .contains("no longer advertised")
    );
    owner.client_mut().apply_snapshot(original).unwrap();
    for result in [
        IntentResult::Rejected {
            reason: "owner confirmation required".into(),
        },
        IntentResult::Stale {
            current_epoch: SceneEpoch(4),
            current_revision: Revision(1),
        },
    ] {
        let progress = owner.submit_action_draft(&target, &mut draft).unwrap();
        assert!(matches!(
            progress,
            Progress::Ask(CarrierRequestBody::Intent(_))
        ));
        let outcome = owner
            .on_response(CarrierResponseBody::Intent(result.clone()))
            .unwrap()
            .done()
            .unwrap();
        let Outcome::Intent(actual) = outcome else {
            panic!("intent outcome")
        };
        assert_eq!(*actual, result);
        assert!(actual.command_outcome().is_some());
    }
}

#[test]
fn old_form_is_not_retargeted_after_owner_epoch_replacement() {
    let (mut view, mut owner, mut snapshot, targets) = fixture();
    view.select("card:2").unwrap();
    let (target, mut draft) = {
        let bound = BoundOwnerActions::new(
            RuntimeSourceBinding::new(view.dataset().source.clone(), targets.clone()),
            &mut owner,
            &snapshot.session,
        )
        .unwrap();
        bound
            .open_action(view.dataset(), view.compiled().unwrap(), INTENT)
            .unwrap()
    };
    draft.choose("choice", "card:2").unwrap();
    snapshot.scene.epoch = SceneEpoch(4);
    snapshot.scene.revision = Revision(1);
    owner.client_mut().apply_snapshot(snapshot.clone()).unwrap();
    let mut bound = BoundOwnerActions::new(
        RuntimeSourceBinding::new(view.dataset().source.clone(), targets),
        &mut owner,
        &snapshot.session,
    )
    .unwrap();
    assert!(
        bound
            .submit_action(
                view.dataset(),
                view.compiled().unwrap(),
                &target,
                &mut draft
            )
            .unwrap_err()
            .contains("target changed")
    );
}

#[test]
fn bound_projection_drives_existing_endpoint_fixture_with_real_mutation_and_refusal() {
    use graphshell_endpoint::{IntentSink, ProjectionCatalog, ProjectionSource};
    use graphshell_host::live_endpoint::{
        ADMITTED_INTENT, COLOURED_INTENT, LiveEndpoint, REFUSED_INTENT,
    };

    let mut endpoint = LiveEndpoint::new();
    let snapshot = endpoint.snapshot(endpoint.request()).unwrap();
    let mut dataset: ProjectionDataset = serde_json::from_str(SOURCE).unwrap();
    dataset.occurrences.truncate(1);
    dataset.source = scenograph::SourceBinding {
        authority: "graphshell-fixture".into(),
        domain: "cards".into(),
        resource: snapshot.session.0.clone(),
    };
    dataset.revision = "fixture-r1".into();
    dataset.occurrences[0].source = snapshot.scene.tables.sources[0].clone().unwrap();
    let mut view =
        ProjectionSession::load(&serde_json::to_string(&dataset).unwrap(), None).unwrap();
    view.select("card:1").unwrap();
    let mut owner = SessionCore::from_descriptor(
        endpoint.describe(),
        CapabilityProfile::new([PresentationCapability::NativeGlyph]),
    );
    let _ = owner.mount(0).unwrap();
    let _ = owner
        .on_response(CarrierResponseBody::Snapshot(Box::new(snapshot.clone())))
        .unwrap();
    let initial_revision = endpoint.current_revision();

    for (index, intent) in [REFUSED_INTENT, COLOURED_INTENT, ADMITTED_INTENT]
        .into_iter()
        .enumerate()
    {
        let request = {
            let mut bound = BoundOwnerActions::new(
                RuntimeSourceBinding::new(
                    view.dataset().source.clone(),
                    BTreeMap::from([("card:1".into(), InstanceId(0))]),
                ),
                &mut owner,
                &snapshot.session,
            )
            .unwrap();
            let (target, mut draft) = bound
                .open_action(view.dataset(), view.compiled().unwrap(), intent)
                .unwrap();
            if let Some(form) = &draft.action().input_form {
                let field = &form.fields[0];
                let (name, value) = (field.name.clone(), field.choices[0].value.clone());
                draft.choose(name, value).unwrap();
            }
            let Progress::Ask(CarrierRequestBody::Intent(request)) = bound
                .submit_action(
                    view.dataset(),
                    view.compiled().unwrap(),
                    &target,
                    &mut draft,
                )
                .unwrap()
            else {
                panic!("intent request")
            };
            request
        };
        let result = endpoint.invoke(request).unwrap();
        match index {
            0 => {
                assert!(matches!(result, IntentResult::Rejected { .. }));
                assert_eq!(endpoint.current_revision(), initial_revision);
            },
            1 => {
                assert_eq!(result, IntentResult::Accepted);
                assert_eq!(
                    endpoint.current_revision(),
                    Revision(initial_revision.0 + 1)
                );
            },
            _ => {
                assert!(matches!(result, IntentResult::Stale { .. }));
                assert_eq!(
                    endpoint.current_revision(),
                    Revision(initial_revision.0 + 1)
                );
            },
        }
        let Outcome::Intent(received) = owner
            .on_response(CarrierResponseBody::Intent(result.clone()))
            .unwrap()
            .done()
            .unwrap()
        else {
            panic!("intent result")
        };
        assert_eq!(*received, result);
    }
}
