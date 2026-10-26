// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The resident inference lane through Djinn's application door (resident
//! inference and recall plan, I1): a resident with the lane on answers an ask
//! over `inference.ask` to Turnstone's label alone, and shuts the lane down;
//! settings naming the lane without a Distillery lane are refused before
//! anything opens.

#![cfg(feature = "inference")]

mod common;
#[path = "fixtures/esp/decoder.rs"]
mod decoder_fixture;

use std::sync::Arc;
use std::time::Duration;

use chirograph::IntentResult;
use common::{
    lane, lending, open_vault, owner, profile, refusal_from, unlock, write_device_settings,
};
use distillery::inference::{
    ASK_INTENT, AskRequestV1, CANCEL_INTENT, CancelRequestV1, INFERENCE_ASK_ROUTE,
    INFERENCE_RUNS_SCHEMA, InferenceProvider, RunStateV1,
};
use djinn::door::app_broker::{AppEndpointCatalog, serve_app_broker};
use djinn::resident::DjinnResident;
use djinn::settings::{InferenceLaneSettings, InferenceModelSettings, OwnerSettings};
use graphshell_endpoint::door::app_admission::{AllowedAppRoutes, AppId, AppRouteGrants};
use graphshell_endpoint::door::app_client::AppClientError;
use graphshell_endpoint::door::route::ResidentEndpointRoute;
use graphshell_host::endpoint_catalog::ResidentEndpointCatalog;

fn stub_lane() -> Option<InferenceLaneSettings> {
    Some(InferenceLaneSettings {
        embedding: None,
        recall: Default::default(),
        model: InferenceModelSettings::Stub,
    })
}

use common::inference_door::{endpoint_name, intent, keeper_host, open, read_runs};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resident_answers_an_ask_on_its_route_and_closes_the_lane() {
    exercise_route(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_library_model_answers_through_the_resident_app_door_without_a_trainer() {
    exercise_route(true).await;
}

async fn save_library(data_root: &std::path::Path) -> eidetic::ManifestId {
    use distillery::inference::{InferenceModelRow, PromptTemplate};
    use eidetic::typed::save_typed;
    use eidetic::{ModelLibrary, PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};
    let path = djinn::resident_distillery::model_library_path(&data_root, &profile().0, None);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut store = muniment::RedbBackend::open(&path).unwrap();
    let model = ModelLibrary::save_model_with_components(
        &mut store,
        "test/tiny-decoder",
        "llama",
        "MIT",
        serde_json::from_str(decoder_fixture::CONFIG_JSON).unwrap(),
        &decoder_fixture::weights_bytes(),
        decoder_fixture::tokenizer_json().as_bytes(),
        vec![],
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap();
    let mut template =
        PromptTemplate::from_bytes(include_bytes!("fixtures/esp/smollm2-instruct-v1.json"))
            .unwrap();
    template.system_prompt.clear();
    template.user.prefix = "t1 ".into();
    template.user.suffix = " t2 ".into();
    template.assistant.prefix = "t3 ".into();
    let template = save_typed(
        &mut store,
        &template,
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap();
    let row = save_typed(
        &mut store,
        &InferenceModelRow { model, template },
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap();
    row
}

async fn exercise_route(library_model: bool) {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());
    write_device_settings(&data_root, Some(lending()));
    let (inference, question, model_id, expected_text, expected_session) = if library_model {
        use distillery::inference::{InferenceDevice, load_library_session};
        let row = save_library(&data_root).await;
        let path = djinn::resident_distillery::model_library_path(&data_root, &profile().0, None);
        let mut store = muniment::RedbBackend::open(&path).unwrap();
        let provider = load_library_session(
            &mut store,
            row,
            &InferenceDevice::ndarray(),
            "burn-ndarray",
            4,
        )
        .await
        .unwrap();
        let prepared = provider.prepare(&Default::default(), "");
        assert!(prepared.is_err(), "empty questions stay invalid");
        let prepared = provider
            .prepare(
                &serde_json::from_value(serde_json::json!({
                    "prompt": "t4", "max_tokens": 4, "temperature": 0.0, "stop": []
                }))
                .unwrap(),
                "",
            )
            .unwrap();
        let expected = provider.bound().generate_prepared(&prepared).unwrap();
        (
            Some(InferenceLaneSettings {
                embedding: None,
                recall: Default::default(),
                model: InferenceModelSettings::Library {
                    weight_cache: None,
                    row: row.to_string(),
                    device: djinn::settings::InferenceDeviceSettings::Cpu,
                    max_tokens: 4,
                },
            }),
            "t4",
            "test/tiny-decoder",
            expected,
            provider.session_identity(),
        )
    } else {
        (
            stub_lane(),
            "what does the lane say?",
            "stub/echo",
            "echo: what does the lane say?".into(),
            None,
        )
    };
    let settings = OwnerSettings {
        inference,
        ..owner(Some(lane()))
    };
    let resident = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        settings,
        &vault_dir,
        unlock(),
    )
    .await
    .expect("open a resident with the inference lane");

    let catalog = AppEndpointCatalog::new(ResidentEndpointCatalog::new());
    let grants = AppRouteGrants::new(AllowedAppRoutes::new([(
        AppId::new("turnstone"),
        ResidentEndpointRoute::new("identity", Duration::from_millis(50)).unwrap(),
    )]));
    let route = catalog
        .update(|catalog| resident.register_inference_route(catalog))
        .await
        .unwrap()
        .expect("the lane registers its route");
    grants.grant(AppId::new("turnstone"), route);

    let endpoint = endpoint_name();
    let server = {
        let (endpoint, grants, catalog) = (endpoint.clone(), grants.clone(), catalog.clone());
        tokio::spawn(async move {
            let _ = serve_app_broker(
                &endpoint,
                Arc::new(djinn::keeper::Keeper::new(keeper_host())),
                grants,
                60_000,
                None,
                catalog,
            )
            .await;
        })
    };
    let mut client = None;
    for _ in 0..100 {
        if let Ok(opened) = open(&endpoint, "turnstone").await {
            client = Some(opened);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut client = client.expect("Turnstone's label is admitted");
    let opened = client.open_session().await.unwrap();
    let request = opened.descriptor.projections[0].request.clone();

    let ask = AskRequestV1 {
        run: "first".into(),
        question: question.into(),
    };
    assert_eq!(
        intent(
            &mut client,
            &request,
            ASK_INTENT,
            serde_json::to_vec(&ask).unwrap()
        )
        .await,
        IntentResult::Accepted
    );
    let mut runs = read_runs(&mut client, &request).await;
    for _ in 0..200 {
        if runs.runs[0].state.is_terminal() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        runs = read_runs(&mut client, &request).await;
    }
    assert_eq!(runs.schema, INFERENCE_RUNS_SCHEMA);
    assert_eq!(runs.model.as_deref(), Some(model_id));
    assert_eq!(runs.session, expected_session);
    assert_eq!(runs.runs[0].session, expected_session);
    assert_eq!(runs.load_error, None);
    if library_model {
        assert!(resident.distillery().unwrap().model_library().is_some());
    }
    assert_eq!(runs.runs[0].run, "first");
    assert_eq!(runs.runs[0].state, RunStateV1::Finished);
    assert_eq!(runs.runs[0].text, expected_text);

    // A cancel of an ended run is accepted and changes nothing; an unknown
    // run and a repeated name are refused.
    let cancel = |run: &str| serde_json::to_vec(&CancelRequestV1 { run: run.into() }).unwrap();
    assert_eq!(
        intent(&mut client, &request, CANCEL_INTENT, cancel("first")).await,
        IntentResult::Accepted
    );
    assert!(matches!(
        intent(&mut client, &request, CANCEL_INTENT, cancel("missing")).await,
        IntentResult::Rejected { .. }
    ));
    assert!(matches!(
        intent(
            &mut client,
            &request,
            ASK_INTENT,
            serde_json::to_vec(&ask).unwrap()
        )
        .await,
        IntentResult::Rejected { .. }
    ));
    if library_model {
        let holder = resident
            .inference_lock_holder()
            .expect("library inference has a lock hook");
        holder.lock();
        assert!(
            client.snapshot(request.clone()).await.is_err(),
            "a previously admitted session cannot snapshot private runs while locked"
        );
        holder.unlock(&identity).unwrap();
        // Some carriers close after a refused request. Reopen through the
        // same app door and confirm the retained answer, not an empty lane.
        let _ = client.close().await;
        client = open(&endpoint, "turnstone").await.unwrap();
        let opened = client.open_session().await.unwrap();
        let unlocked_request = opened.descriptor.projections[0].request.clone();
        let unlocked = read_runs(&mut client, &unlocked_request).await;
        assert_eq!(unlocked.runs[0].text, expected_text);
        assert_eq!(unlocked.runs[0].state, RunStateV1::Finished);
    }
    client.close().await.unwrap();

    // Granted to Turnstone alone (ruling 8).
    for app in ["knot-editor", "djinn", "someone-elses-app"] {
        assert!(
            matches!(open(&endpoint, app).await, Err(AppClientError::Closed)),
            "{app} must be refused {INFERENCE_ASK_ROUTE}"
        );
    }

    server.abort();
    let _ = server.await;
    drop(catalog);
    resident
        .shutdown()
        .await
        .expect("the resident, inference lane first, shuts down cleanly");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_lane_without_a_distillery_lane_is_refused_before_anything_opens() {
    let directory = tempfile::tempdir().expect("temporary resident root");
    let data_root = directory.path().join("data");
    let (vault_dir, identity) = open_vault(directory.path());
    let settings = OwnerSettings {
        inference: stub_lane(),
        ..owner(None)
    };
    let refusal = refusal_from(
        DjinnResident::open(
            &identity,
            &data_root,
            &profile(),
            settings,
            &vault_dir,
            unlock(),
        )
        .await,
        "inference without a distillery lane must be refused",
    )
    .await;
    assert!(refusal.contains("needs the distillery lane"), "{refusal}");
}

/// Two production resident compositions, with independent vaults, one graph
/// key agreement and the actual personal-sync endpoints. The tiny decoder is
/// only the model fixture; no identity seed or data key is copied between them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn keyed_personal_graph_devices_continue_a_seeded_ask() {
    use distillery::inference::{InferenceDevice, RunJournal, load_library_session};
    use djinn::sync::personal_sync_host::{PersonalSyncHost, PersonalSyncHostConfig};
    use graphshell::personal_sync::{SyncRoster, SyncSelection};
    use p2panda_core::{Body, Header, Operation, SigningKey};
    use personae::IdentityProvider;

    let temp = tempfile::tempdir().unwrap();
    let root_a = temp.path().join("a");
    let root_b = temp.path().join("b");
    std::fs::create_dir_all(&root_a).unwrap();
    std::fs::create_dir_all(&root_b).unwrap();
    let (vault_a, identity_a) = open_vault(&root_a);
    let (vault_b, identity_b) = open_vault(&root_b);
    assert_ne!(
        identity_a.master_public_key(),
        identity_b.master_public_key()
    );
    let data_a = root_a.join("data");
    let data_b = root_b.join("data");
    write_device_settings(&data_a, Some(lending()));
    write_device_settings(&data_b, Some(lending()));
    let row = save_library(&data_a).await;
    assert_eq!(save_library(&data_b).await, row);
    let settings = OwnerSettings {
        inference: Some(InferenceLaneSettings {
            embedding: None,
            recall: Default::default(),
            model: InferenceModelSettings::Library {
                weight_cache: None,
                row: row.to_string(),
                device: djinn::settings::InferenceDeviceSettings::Cpu,
                max_tokens: 8,
            },
        }),
        ..owner(Some(lane()))
    };
    let local = DjinnResident::open(
        &identity_a,
        &data_a,
        &profile(),
        settings.clone(),
        &vault_a,
        unlock(),
    )
    .await
    .unwrap();
    local
        .inference()
        .unwrap()
        .lane()
        .ask(AskRequestV1 {
            run: "retained-local".into(),
            question: "t4".into(),
        })
        .unwrap();
    let local_finished = wait_run(
        local.inference().unwrap().lane(),
        "retained-local",
        |run| run.state.is_terminal(),
        false,
    )
    .await;
    assert_eq!(local_finished.state, RunStateV1::Finished);
    local.shutdown().await.unwrap();
    let mut a = DjinnResident::open_deferred_inference(
        &identity_a,
        &data_a,
        &profile(),
        settings.clone(),
        &vault_a,
        unlock(),
    )
    .await
    .unwrap();
    let mut b = DjinnResident::open_deferred_inference(
        &identity_b,
        &data_b,
        &profile(),
        settings.clone(),
        &vault_b,
        unlock(),
    )
    .await
    .unwrap();
    assert!(
        a.inference().is_none(),
        "no early actor can resume local history during composition"
    );
    let graph = [0x67; 32];
    let roster = SyncRoster::new([
        identity_a.master_public_key().to_bytes(),
        identity_b.master_public_key().to_bytes(),
    ]);
    let config = |root: &std::path::Path, tickets: Vec<String>| PersonalSyncHostConfig {
        graph,
        store_path: root.join("personal.redb"),
        roster: roster.clone(),
        selection: SyncSelection::default(),
        peer_tickets: tickets,
        peer_hints: Vec::new(),
        paired_nodes: Vec::new(),
        relay_urls: Vec::new(),
    };
    let custody_a = a.blobs();
    let custody_b = b.blobs();
    custody_a
        .bind_scope(transport::BlobScope::new(graph))
        .await
        .unwrap();
    custody_b
        .bind_scope(transport::BlobScope::new(graph))
        .await
        .unwrap();
    let host_a = Arc::new(
        PersonalSyncHost::open_with_blob_custody(
            &identity_a,
            config(&root_a, vec![]),
            custody_a.blobs(),
            custody_a.authorizer(),
        )
        .await
        .unwrap(),
    );
    let host_b = Arc::new(
        PersonalSyncHost::open_with_blob_custody(
            &identity_b,
            config(&root_b, vec![host_a.ticket().await.unwrap()]),
            custody_b.blobs(),
            custody_b.authorizer(),
        )
        .await
        .unwrap(),
    );
    drop(custody_a);
    drop(custody_b);
    assert_ne!(host_a.node_id(), host_b.node_id());
    // Register the private topic before pairing A's peer, exercising the live
    // append path as well as B's already-known ticket path.
    a.start_inference(&identity_a, Some(host_a.clone()))
        .await
        .unwrap();
    b.start_inference(&identity_b, Some(host_b.clone()))
        .await
        .unwrap();
    host_a.pair_node(host_b.node_id()).await.unwrap();
    let lane_a = a.inference().unwrap().lane().clone();
    let lane_b = b.inference().unwrap().lane().clone();
    let journal_a = a.inference().unwrap().journal().unwrap().clone();
    let journal_b = b.inference().unwrap().journal().unwrap().clone();
    let ask = |run: &str| AskRequestV1 {
        run: run.into(),
        question: "t4".into(),
    };
    assert!(
        lane_b
            .ask(ask("unkeyed"))
            .unwrap_err()
            .to_string()
            .contains("keyed reader")
    );
    assert!(journal_b.operations().await.unwrap().is_empty());
    host_a.enable_encryption().await.unwrap();
    host_a
        .admit_reader(identity_b.master_public_key().to_bytes(), "device b")
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        host_a.key_paired_devices().await.unwrap();
        host_b.key_paired_devices().await.unwrap();
        if host_b.seals().can_write() {
            let probe = host_a.seals().seal(b"same graph epoch").unwrap();
            if host_b.seals().open(&probe).is_ok() {
                break;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the existing graph agreement did not key B"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Existing local history is re-sealed into the keyed graph without
    // overwriting an existing shared head or handing B the local data key.
    lane_a.refresh_retained_runs().unwrap();
    let imported = wait_run(
        &lane_b,
        "retained-local",
        |run| run.state == RunStateV1::Finished,
        true,
    )
    .await;
    assert_eq!(imported.text, local_finished.text);
    // The resident owns that redb, so use its existing handle for the reference.
    let library = a.distillery().unwrap().model_library().unwrap();
    let reference = load_library_session(
        &mut *library.lock().await,
        row,
        &InferenceDevice::ndarray(),
        "burn-ndarray",
        8,
    )
    .await
    .unwrap();
    let request = serde_json::from_value(serde_json::json!({
        "prompt":"t4", "max_tokens":8, "temperature":0.85,
        "top_p":0.95, "seed":42, "stop":[]
    }))
    .unwrap();
    let expected = reference
        .generate_checkpointed(&request, None, &mut |_| Ok(()), &mut || false)
        .unwrap();
    assert_eq!(expected.token_ids.len(), 8);
    let pause_lane = lane_a.clone();
    let pauser = tokio::task::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            if pause_lane
                .runs()
                .runs
                .iter()
                .any(|run| run.run == "across" && run.tokens >= 3)
            {
                pause_lane.pause("across", "user").unwrap();
                break;
            }
            assert!(std::time::Instant::now() < deadline, "no committed prefix");
            std::thread::yield_now();
        }
    });
    lane_a.ask_with_generation(ask("across"), request).unwrap();
    pauser.await.unwrap();
    let paused = wait_run(
        &lane_a,
        "across",
        |run| run.state == RunStateV1::Paused,
        false,
    )
    .await;
    assert!((3..8).contains(&paused.tokens));
    // A remote materialization is locally paused even while its source still
    // reports Running. Wait for the signed pause record, not that projection,
    // so this is a sequential handoff rather than a concurrent continuation.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match journal_b.retained_record("across") {
            Ok(record)
                if record.status == eidetic::models::inference::InferenceRunStatus::Paused =>
            {
                break;
            },
            Ok(_) => {},
            Err(error) => assert!(
                error.contains("missing parent") || error.contains("unknown retained run"),
                "{error}"
            ),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the durable pause did not reach B"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let remote = wait_run(
        &lane_b,
        "across",
        |run| run.state == RunStateV1::Paused && run.tokens == paused.tokens,
        true,
    )
    .await;
    assert_eq!(remote.text, paused.text);
    lane_b.resume("across", "device-transfer").unwrap();
    let finished = wait_run(&lane_b, "across", |run| run.state.is_terminal(), false).await;
    assert_eq!(finished.state, RunStateV1::Finished, "{:?}", finished.error);
    assert_eq!(finished.text, expected.text);
    assert_eq!(finished.session, paused.session);
    assert_eq!(
        finished.continuations.last().unwrap().device,
        journal_b.device()
    );
    assert_eq!(
        finished.continuations.last().unwrap().at_token,
        paused.tokens
    );
    let evidence = journal_b.retained_record("across").unwrap();
    let actual: serde_json::Value = serde_json::from_slice(
        &journal_b
            .resolve_evidence_blob(evidence.checkpoint_blob.unwrap())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(actual["token_ids"], serde_json::json!(expected.token_ids));
    assert_eq!(
        journal_b.record_comparison("across", &expected).unwrap(),
        None
    );

    // The selected boundary is the key group, independently of permission to
    // write the browser graph itself. Removing B from that write roster alone
    // must not revoke its still-current inference seat.
    host_a
        .set_roster(SyncRoster::new([identity_a.master_public_key().to_bytes()]))
        .await;
    let sibling_key = SigningKey::from_bytes(
        &identity_b
            .derive_keypair(&{
                let mut salt = b"mere.djinn/personal-inference/writer/v1/".to_vec();
                salt.extend_from_slice(&graph);
                salt
            })
            .unwrap()
            .to_seed(),
    );
    // The journal rejects an operation whose checkpoint parent it has not
    // seen, so carry the sibling's whole chain in order, not only its tip.
    let sibling_operations: Vec<_> = journal_b
        .operations()
        .await
        .unwrap()
        .into_iter()
        .filter(|op| op.header.verifying_key == sibling_key.verifying_key())
        .collect();
    assert!(!sibling_operations.is_empty(), "the sibling wrote nothing");
    for operation in &sibling_operations {
        journal_a.accept(operation).await.unwrap();
    }
    host_a.set_roster(roster.clone()).await;

    wait_run(
        &lane_a,
        "across",
        |run| run.state == RunStateV1::Finished,
        true,
    )
    .await;
    let mut salt = b"mere.djinn/personal-inference/writer/v1/".to_vec();
    salt.extend_from_slice(&graph);
    let owner_key = SigningKey::from_bytes(&identity_a.derive_keypair(&salt).unwrap().to_seed());
    let run_hash = *p2panda_core::Hash::digest(b"across").as_bytes();
    let operation = journal_a
        .operations()
        .await
        .unwrap()
        .into_iter()
        .filter(|op| {
            op.header.extensions.run == run_hash
                && op.header.verifying_key == owner_key.verifying_key()
        })
        .last()
        .unwrap();
    let mut ext = operation.header.extensions.clone();
    ext.space = [0x68; 32];
    let body = operation.body.as_ref().unwrap().as_bytes();
    let header = Header::builder().body(body).build(&owner_key, ext);
    let wrong_graph = Operation {
        hash: header.hash(),
        header,
        body: Some(Body::from_bytes(body)),
    };
    assert!(
        journal_b
            .accept(&wrong_graph)
            .await
            .unwrap_err()
            .contains("wrong-space")
    );
    // Possession of the endpoint or even the sealing handle cannot replace
    // this graph's admitted root and domain-bound writer attestation.
    let outsider = personae::InMemoryProvider::from_seed([0x69; 32]);
    let mut ext = operation.header.extensions.clone();
    ext.authority = djinn::resident_inference::personal::writer_authority(
        &outsider.attest_derived_key(&salt).unwrap(),
    )
    .unwrap();
    let key = SigningKey::from_bytes(&outsider.derive_keypair(&salt).unwrap().to_seed());
    let body = operation.body.as_ref().unwrap().as_bytes();
    // Re-seal the matching address, so the membership test cannot accidentally
    // pass merely because an unrelated envelope failed address binding.
    let sealed = serde_json::from_slice(body).unwrap();
    let mut protected: serde_json::Value =
        serde_json::from_slice(&host_a.seals().open(&sealed).unwrap()).unwrap();
    protected["address"] = serde_json::to_value(&ext).unwrap();
    let outside_body = serde_json::to_vec(
        &host_a
            .seals()
            .seal(&serde_json::to_vec(&protected).unwrap())
            .unwrap(),
    )
    .unwrap();
    let header = Header::builder().body(&outside_body).build(&key, ext);
    let forbidden = Operation {
        hash: header.hash(),
        header,
        body: Some(Body::from_bytes(&outside_body)),
    };
    assert!(
        journal_b
            .accept(&forbidden)
            .await
            .unwrap_err()
            .contains("key group")
    );
    // A valid signer also cannot transplant an envelope to another ask.
    let mut ext = operation.header.extensions.clone();
    ext.run = [0x6a; 32];
    let header = Header::builder().body(body).build(&owner_key, ext);
    let transplanted = Operation {
        hash: header.hash(),
        header,
        body: Some(Body::from_bytes(body)),
    };
    assert!(
        journal_b
            .accept(&transplanted)
            .await
            .unwrap_err()
            .contains("another domain or log")
    );

    let plaintext = host_a
        .seals()
        .open(&serde_json::from_slice(body).unwrap())
        .unwrap();
    let header = Header::builder()
        .body(&plaintext)
        .build(&owner_key, operation.header.extensions.clone());
    let unsealed = Operation {
        hash: header.hash(),
        header,
        body: Some(Body::from_bytes(&plaintext)),
    };
    assert!(
        journal_b.accept(&unsealed).await.is_err(),
        "private asks have no plaintext fallback"
    );
    let mut ext = operation.header.extensions.clone();
    ext.authority = djinn::resident_inference::personal::writer_authority(
        &identity_a
            .attest_derived_key(b"wrong inference domain")
            .unwrap(),
    )
    .unwrap();
    let header = Header::builder().body(body).build(&owner_key, ext);
    let wrong_domain = Operation {
        hash: header.hash(),
        header,
        body: Some(Body::from_bytes(body)),
    };
    assert!(journal_b.accept(&wrong_domain).await.is_err());

    let other_parent = journal_b
        .operations()
        .await
        .unwrap()
        .into_iter()
        .find(|op| op.header.extensions.run != run_hash)
        .unwrap();
    let mut protected: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    let payload: Vec<u8> = serde_json::from_value(protected["payload"].clone()).unwrap();
    let mut delta: serde_json::Value = serde_json::from_slice(&payload).unwrap();
    delta["parent"] = serde_json::json!(other_parent.hash.to_string());
    protected["payload"] = serde_json::to_value(serde_json::to_vec(&delta).unwrap()).unwrap();
    let cross_body = serde_json::to_vec(
        &host_a
            .seals()
            .seal(&serde_json::to_vec(&protected).unwrap())
            .unwrap(),
    )
    .unwrap();
    let header = Header::builder()
        .body(&cross_body)
        .build(&owner_key, operation.header.extensions.clone());
    let cross_run = Operation {
        hash: header.hash(),
        header,
        body: Some(Body::from_bytes(&cross_body)),
    };
    assert!(
        journal_b
            .accept(&cross_run)
            .await
            .unwrap_err()
            .contains("parent addresses another run")
    );

    let holder = b.inference_lock_holder().unwrap();
    holder.lock();
    assert!(journal_b.load().is_err());
    assert!(
        journal_b
            .accept(&operation)
            .await
            .unwrap_err()
            .contains("locked")
    );
    lane_a.ask(ask("during-lock")).unwrap();
    wait_run(&lane_a, "during-lock", |run| run.state.is_terminal(), false).await;
    for op in journal_a.operations().await.unwrap().iter().filter(|op| {
        op.header.extensions.run == *p2panda_core::Hash::digest(b"during-lock").as_bytes()
    }) {
        assert!(journal_b.accept(op).await.unwrap_err().contains("locked"));
    }
    holder.unlock(&identity_b).unwrap();
    wait_run(
        &lane_b,
        "during-lock",
        |run| run.state == RunStateV1::Finished,
        true,
    )
    .await;
    assert_eq!(
        journal_b.retained_record("across").unwrap().status,
        eidetic::models::inference::InferenceRunStatus::Finished
    );
    // Removal retains old readable history, while the graph's actual epoch
    // rotation excludes this device from the next private ask.
    host_a
        .retire_reader(identity_b.master_public_key().to_bytes())
        .await
        .unwrap();
    host_a.key_paired_devices().await.unwrap();
    lane_a.ask(ask("after-revocation")).unwrap();
    wait_run(
        &lane_a,
        "after-revocation",
        |run| run.state.is_terminal(),
        false,
    )
    .await;
    let new_ops = journal_a.operations().await.unwrap();
    let after = new_ops
        .iter()
        .find(|op| {
            op.header.extensions.run == *p2panda_core::Hash::digest(b"after-revocation").as_bytes()
        })
        .unwrap();
    assert!(
        journal_b.accept(after).await.is_err(),
        "the departed reader must not read the new epoch"
    );
    assert!(journal_b.retained_record("across").is_ok());
    host_a
        .set_roster(SyncRoster::new([identity_a.master_public_key().to_bytes()]))
        .await;
    assert!(
        journal_a.retained_record("across").is_ok(),
        "an admitted historical continuation survives removal of its writer"
    );
    // A fresh journal with the current graph keys can still admit the old
    // reader's historical epoch after that reader has left the current group.
    let late_root = temp.path().join("late-view");
    std::fs::create_dir_all(&late_root).unwrap();
    let local = Arc::new(distillery::inference::durable::PersonaRunJournal::new(
        muniment::RedbBackend::open(late_root.join("local.redb")).unwrap(),
        [0x70; 32],
        &identity_a.derive_keypair(&salt).unwrap(),
        "late-view".into(),
        [0x71; 32],
    ));
    let late = djinn::resident_inference::personal::open(
        &identity_a,
        &late_root,
        &host_a,
        local,
        [0x71; 32],
    )
    .unwrap();
    let history = journal_b.operations().await.unwrap();
    let orphan = history
        .iter()
        .find(|op| op.header.verifying_key != owner_key.verifying_key() && op.header.seq_num == 0)
        .unwrap();
    assert!(
        late.accept(orphan)
            .await
            .unwrap_err()
            .contains("missing checkpoint parent")
    );
    assert!(late.operations().await.unwrap().is_empty());
    assert!(
        late.load().unwrap().is_empty(),
        "an out-of-order child cannot poison restart"
    );
    for _ in 0..=history.len() {
        for op in &history {
            if let Err(error) = late.accept(op).await {
                assert!(error.contains("missing checkpoint parent"), "{error}");
            }
        }
        if late.operations().await.unwrap().len() == history.len() {
            break;
        }
    }
    assert_eq!(late.operations().await.unwrap().len(), history.len());
    assert_eq!(
        late.retained_record("across").unwrap().status,
        eidetic::models::inference::InferenceRunStatus::Finished
    );
    let former = history
        .iter()
        .find(|op| op.header.verifying_key != owner_key.verifying_key())
        .unwrap();
    let frame = serde_json::from_slice(former.body.as_ref().unwrap().as_bytes()).unwrap();
    let clear = host_a.seals().open(&frame).unwrap();
    let current_body = serde_json::to_vec(&host_a.seals().seal(&clear).unwrap()).unwrap();
    let former_key = SigningKey::from_bytes(&identity_b.derive_keypair(&salt).unwrap().to_seed());
    let header = Header::builder()
        .body(&current_body)
        .build(&former_key, former.header.extensions.clone());
    let forbidden_current = Operation {
        hash: header.hash(),
        header,
        body: Some(Body::from_bytes(&current_body)),
    };
    assert!(
        late.accept(&forbidden_current)
            .await
            .unwrap_err()
            .contains("key group")
    );
    late.shutdown().await.unwrap();
    drop(late);
    // An independently named local ask must not disappear behind an existing
    // shared run with the same name. Keep its source and report the collision.
    let collision_source = Arc::new(distillery::inference::durable::PersonaRunJournal::new(
        muniment::RedbBackend::open(late_root.join("collision-local.redb")).unwrap(),
        [0x70; 32],
        &identity_a.derive_keypair(&salt).unwrap(),
        "another-device".into(),
        [0x71; 32],
    ));
    let mut conflicting = serde_json::to_value(
        journal_b
            .load()
            .unwrap()
            .into_iter()
            .find(|record| serde_json::to_value(record).unwrap()["view"]["run"] == "across")
            .unwrap(),
    )
    .unwrap();
    conflicting["view"]["question"] = serde_json::json!("a different retained question");
    conflicting["request"]["prompt"] = serde_json::json!("a different retained question");
    conflicting["view"]["text"] = serde_json::json!("");
    conflicting["view"]["state"] = serde_json::json!("queued");
    conflicting["view"]["session"] = serde_json::Value::Null;
    conflicting["checkpoint"] = serde_json::Value::Null;
    conflicting["continuations"] = serde_json::json!([{
        "device":"another-device", "at_token":0, "reason":"ask",
        "first_differing_token":null, "compared_token_ids_hash":null
    }]);
    let conflicting = serde_json::from_value(conflicting).unwrap();
    collision_source.append(&conflicting).unwrap();
    let collision = djinn::resident_inference::personal::open(
        &identity_a,
        &late_root,
        &host_a,
        collision_source.clone(),
        [0x71; 32],
    )
    .unwrap();
    let collision_error = collision
        .load()
        .err()
        .expect("a conflicting local ask must remain visible");
    assert!(collision_error.contains("collides with personal graph history"));
    assert_eq!(
        serde_json::to_value(&collision_source.load().unwrap()[0]).unwrap()["view"]["question"],
        "a different retained question"
    );
    drop(collision);
    drop(collision_source);
    let receipt = serde_json::json!({
        "separate_identity_roots":true, "separate_transport_nodes":true,
        "keyed_reader_need_not_write_graph":true,
        "model_row":row.to_string(), "session":finished.session,
        "paused_tokens":paused.tokens, "completed_tokens":finished.tokens,
        "seed":42, "resumed_token_ids":actual["token_ids"],
        "reference_token_ids":expected.token_ids, "first_differing_token":null,
        "continuations":finished.continuations,
        "retained_evidence":journal_b.retained_record("across").unwrap(),
        "unkeyed_refused":true, "wrong_graph_refused":true,
        "outside_key_group_refused":true, "transplanted_envelope_refused":true,
        "plaintext_fallback_refused":true, "wrong_writer_domain_refused":true,
        "locked_refused":true, "sync_catches_up_after_unlock":true,
        "new_epoch_excludes_retired_reader":true,
        "retired_reader_retains_history":true,
        "local_history_imported":true, "conflicting_local_history_preserved":true,
        "retained_history_survives_roster_removal":true,
        "late_history_after_revocation":true, "former_reader_current_epoch_write_refused":true,
        "out_of_order_child_refused_before_persistence":true, "cross_run_parent_refused":true,
        "a_sync_ops_received":journal_a.sync_status().unwrap().ops_received,
        "b_sync_ops_received":journal_b.sync_status().unwrap().ops_received,
        "boundary":"two independently rooted resident compositions on one computer; actual personal graph key agreement and endpoints; tiny CPU model; no installed binary or physical second computer"
    });
    drop(reference);
    drop(library);
    drop(holder);
    drop(lane_a);
    drop(lane_b);
    drop(host_a);
    drop(host_b);
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
    journal_a.shutdown().await.unwrap();
    journal_b.shutdown().await.unwrap();
    drop(journal_a);
    drop(journal_b);
    drop(muniment::RedbBackend::open(root_a.join("personal.redb")).unwrap());
    drop(muniment::RedbBackend::open(root_b.join("personal.redb")).unwrap());
    let mut receipt = receipt;
    receipt["shared_custody_shutdown_drained"] = serde_json::json!(true);
    if let Some(path) = std::env::var_os("DJINN_PERSONAL_INFERENCE_RECEIPT") {
        std::fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
}

async fn wait_run(
    lane: &distillery::inference::InferenceLane,
    id: &str,
    test: impl Fn(&distillery::inference::RunV1) -> bool,
    refresh: bool,
) -> distillery::inference::RunV1 {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if refresh {
            if let Err(error) = lane.refresh_retained_runs() {
                assert!(error.contains("missing parent"), "{error}");
            }
        }
        if let Some(run) = lane.runs().runs.into_iter().find(|run| run.run == id) {
            if test(&run) {
                return run;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out: {:?}",
            lane.runs()
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// I4 qualification: the pinned trained model, retained versioned material,
/// actual Turnstone app connection, and a sealed run reopened after restart.
#[cfg(feature = "inference-gpu")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires pinned ESP_SMOLLM2_DIR and a discrete GPU"]
async fn a_real_model_cites_exact_retained_versions_and_reopens_its_run() {
    use distillery::inference::recall::{
        PageTextCorpus, RECALL_ASK_INTENT, RecallAskRequestV1, RecallConfig,
    };
    use eidetic::models::recall::{RecallSourceId, RecallSourceKind};

    let temp = tempfile::tempdir().unwrap();
    let data_root = temp.path().join("data");
    let (vault_dir, identity) = open_vault(temp.path());
    write_device_settings(&data_root, Some(lending()));
    let row = save_pinned_answer_model(&data_root).await;
    let settings = OwnerSettings {
        inference: Some(InferenceLaneSettings {
            embedding: None,
            recall: Default::default(),
            model: InferenceModelSettings::Library {
                weight_cache: None,
                row: row.to_string(),
                device: djinn::settings::InferenceDeviceSettings::Gpu,
                max_tokens: 64,
            },
        }),
        ..owner(Some(lane()))
    };
    let resident = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        settings.clone(),
        &vault_dir,
        unlock(),
    )
    .await
    .unwrap();
    let source_store =
        Arc::new(muniment::RedbBackend::open(temp.path().join("retained.redb")).unwrap());
    let source_a = "https://retained.test/pelican";
    let text_a =
        "The Pelican observatory's access word is amberstone. It has no other access word.";
    let text_b = "The Sandpiper observatory's access word is indigo. It has no other access word.";
    let version_a = eidetic::PageTextStore::new(source_store.as_ref())
        .put(source_a, text_a, 1)
        .await
        .unwrap();
    eidetic::PageTextStore::new(source_store.as_ref())
        .put("https://retained.test/sandpiper", text_b, 1)
        .await
        .unwrap();
    let corpus = Arc::new(PageTextCorpus(source_store.clone()));
    let recall_config = RecallConfig {
        max_passages: 1,
        ..Default::default()
    };
    resident
        .configure_inference_recall(corpus.clone(), recall_config.clone())
        .unwrap();
    let lane = resident.inference().unwrap().lane();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    while lane.runs().session.is_none() {
        assert!(lane.runs().load_error.is_none(), "{:?}", lane.runs());
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let catalog = AppEndpointCatalog::new(ResidentEndpointCatalog::new());
    let grants = AppRouteGrants::new(AllowedAppRoutes::new([]));
    let route = catalog
        .update(|catalog| resident.register_inference_route(catalog))
        .await
        .unwrap()
        .unwrap();
    grants.grant(AppId::new("turnstone"), route);
    let endpoint = endpoint_name();
    let server = {
        let endpoint = endpoint.clone();
        let grants = grants.clone();
        let catalog = catalog.clone();
        tokio::spawn(async move {
            let _ = serve_app_broker(
                &endpoint,
                Arc::new(djinn::keeper::Keeper::new(keeper_host())),
                grants,
                60_000,
                None,
                catalog,
            )
            .await;
        })
    };
    let mut client = None;
    for _ in 0..100 {
        if let Ok(opened) = open(&endpoint, "turnstone").await {
            client = Some(opened);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut client = client.unwrap();
    let opened = client.open_session().await.unwrap();
    let request = opened.descriptor.projections[0].request.clone();
    let source = RecallSourceId {
        kind: RecallSourceKind::PageText,
        source: source_a.into(),
        owner: None,
        node: None,
    };
    let ask=RecallAskRequestV1 { answer_format: Default::default(), ask:AskRequestV1 { run:"retained-answer".into(),question:"What is the Pelican observatory's access word? Answer in one short sentence and cite the passage using [1].".into() },scope:vec![] };
    assert_eq!(
        intent(
            &mut client,
            &request,
            RECALL_ASK_INTENT,
            serde_json::to_vec(&ask).unwrap()
        )
        .await,
        IntentResult::Accepted
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let answered = loop {
        let runs = read_runs(&mut client, &request).await;
        if runs.runs[0].state.is_terminal() {
            break runs.runs[0].clone();
        }
        assert!(tokio::time::Instant::now() < deadline, "{:?}", runs);
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(answered.state, RunStateV1::Finished, "{:?}", answered);
    assert!(
        answered.text.to_lowercase().contains("amberstone"),
        "{}",
        answered.text
    );
    let recalled = answered.recall.as_ref().unwrap();
    assert_eq!(recalled.passages.len(), 1);
    assert_eq!(recalled.citations.len(), 1, "{}", answered.text);
    assert!(recalled.unknown_citations.is_empty());
    let supplied = &recalled.passages[0];
    assert_eq!(supplied.passage.source, source);
    assert_eq!(
        supplied.passage.version,
        format!("blake3:{}", version_a.to_hex())
    );
    assert_eq!(supplied.passage.text, text_a);
    assert_eq!(supplied.passage.selector.start, 0);
    assert_eq!(supplied.passage.selector.end, text_a.chars().count());
    assert_eq!(recalled.citations[0].passage, supplied.passage.id());
    if recalled.citations[0].basis == distillery::inference::recall::CitationBasis::VerbatimPassage
    {
        let span = &recalled.citations[0].answer_selector;
        assert_eq!(
            answered
                .text
                .chars()
                .skip(span.start)
                .take(span.end - span.start)
                .collect::<String>(),
            text_a
        );
    } else {
        assert!(answered.text.contains("[1]"));
    }

    let journal = resident.inference().unwrap().journal().unwrap();
    let evidence = journal.retained_record("retained-answer").unwrap();
    assert_eq!(evidence.supplied_passages, vec![supplied.passage.id()]);
    assert_eq!(
        journal
            .resolve_passage(evidence.supplied_passages[0])
            .unwrap()
            .unwrap(),
        supplied.passage
    );
    let checkpoint = journal
        .resolve_evidence_blob(evidence.checkpoint_blob.unwrap())
        .unwrap()
        .unwrap();
    let checkpoint: serde_json::Value = serde_json::from_slice(&checkpoint).unwrap();
    let prompt = checkpoint["request"]["prompt"].as_str().unwrap();
    assert!(prompt.contains(text_a));
    assert!(!prompt.contains(&supplied.passage.version));
    assert!(!prompt.contains(&supplied.passage.source.source));
    assert!(prompt.contains("[1]"));
    assert!(!prompt.contains(text_b));
    let source_revision_before = lane.generation();
    let changed = eidetic::PageTextStore::new(source_store.as_ref())
        .put(
            source_a,
            "The Pelican observatory's access word is now violet.",
            2,
        )
        .await
        .unwrap();
    assert!(
        lane.generation() > source_revision_before,
        "source changes must produce a route notice revision"
    );
    let stale = read_runs(&mut client, &request).await.runs.remove(0);
    assert!(stale.recall.as_ref().unwrap().citations.is_empty());
    assert_eq!(
        stale.recall.as_ref().unwrap().stale[0].current_version,
        Some(format!("blake3:{}", changed.to_hex()))
    );
    let mut changed_ask = ask.clone();
    changed_ask.ask.run = "changed-source".into();
    changed_ask.scope = vec![source.clone()];
    assert_eq!(
        intent(
            &mut client,
            &request,
            RECALL_ASK_INTENT,
            serde_json::to_vec(&changed_ask).unwrap()
        )
        .await,
        IntentResult::Accepted
    );
    let blocked = read_runs(&mut client, &request)
        .await
        .runs
        .into_iter()
        .find(|run| run.run == "changed-source")
        .unwrap();
    assert_eq!(blocked.state, RunStateV1::Failed);
    assert_eq!(blocked.tokens, 0);
    assert!(blocked.error.as_ref().unwrap().contains("sources changed"));
    assert!(blocked.recall.as_ref().unwrap().passages.is_empty());
    assert!(blocked.recall.as_ref().unwrap().citations.is_empty());
    assert_eq!(
        blocked.recall.as_ref().unwrap().stale[0].indexed_version,
        supplied.passage.version
    );
    let failed_evidence = journal.retained_record("changed-source").unwrap();
    assert!(failed_evidence.supplied_passages.is_empty());
    assert!(failed_evidence.checkpoint_blob.is_none());
    let holder = resident.inference_lock_holder().unwrap();
    holder.lock();
    assert!(client.snapshot(request.clone()).await.is_err());
    holder.unlock(&identity).unwrap();
    drop(holder);
    let _ = client.close().await;
    server.abort();
    let _ = server.await;
    drop(catalog);
    resident.shutdown().await.unwrap();
    let reopened = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        settings,
        &vault_dir,
        unlock(),
    )
    .await
    .unwrap();
    reopened
        .configure_inference_recall(corpus, recall_config)
        .unwrap();
    let past = reopened
        .inference()
        .unwrap()
        .lane()
        .runs()
        .runs
        .into_iter()
        .find(|run| run.run == "retained-answer")
        .unwrap();
    assert_eq!(past.state, RunStateV1::Finished);
    assert_eq!(past.text, answered.text);
    assert!(past.recall.as_ref().unwrap().citations.is_empty());
    let after = reopened
        .inference()
        .unwrap()
        .journal()
        .unwrap()
        .retained_record("retained-answer")
        .unwrap();
    assert_eq!(after, evidence);
    assert_eq!(
        reopened
            .inference()
            .unwrap()
            .journal()
            .unwrap()
            .resolve_passage(after.supplied_passages[0])
            .unwrap()
            .unwrap(),
        supplied.passage
    );
    reopened.shutdown().await.unwrap();
    let receipt = serde_json::json!({"model_row":row,"answer":answered,"after_source_change":stale,"reopened":past,"retained_record":evidence,"rendered_prompt":prompt,"checkpoint_token_ids":checkpoint["token_ids"],"source_change_advanced_revision":true,"stale_source_blocked_before_generation":true,"stale_run":blocked,"stale_run_evidence":failed_evidence,"lock_refused":true,"exact_native_passage_reopened":true,"real_model":true,"actual_app_connection":true,"boundary":"native in-process Djinn, actual app connection, pinned SmolLM2 on GPU, separately owned retained redb source; no installed binary or headed application"});
    if let Some(path) = std::env::var_os("DJINN_RECALL_RECEIPT") {
        std::fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
}

/// Save the byte-pinned public checkpoint and its declared prompt template.
#[cfg(feature = "inference-gpu")]
async fn save_pinned_answer_model(data_root: &std::path::Path) -> eidetic::ManifestId {
    use distillery::inference::{InferenceModelRow, PromptTemplate};
    use eidetic::typed::save_typed;
    use eidetic::{ModelLibrary, PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};
    use sha2::{Digest, Sha256};
    let directory =
        std::path::PathBuf::from(std::env::var_os("ESP_SMOLLM2_DIR").expect("ESP_SMOLLM2_DIR"));
    let reference: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/esp/smollm2_360m_reference.json")).unwrap();
    let read = |name: &str| {
        let bytes = std::fs::read(directory.join(name)).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            reference["artifacts"][name].as_str().unwrap(),
            "exact {name}"
        );
        bytes
    };
    let path = djinn::resident_distillery::model_library_path(&data_root, &profile().0, None);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut library = muniment::RedbBackend::open(&path).unwrap();
    let config = read("config.json");
    let tokenizer = read("tokenizer.json");
    let weights = read("model.safetensors");
    read("tokenizer_config.json");
    let model = ModelLibrary::save_model_with_components(
        &mut library,
        reference["model_id"].as_str().unwrap(),
        "llama",
        "Apache-2.0",
        serde_json::from_slice(&config).unwrap(),
        &weights,
        &tokenizer,
        vec![],
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap();
    drop(weights);
    let template =
        PromptTemplate::from_bytes(include_bytes!("fixtures/esp/smollm2-instruct-v1.json"))
            .unwrap();
    let template = save_typed(
        &mut library,
        &template,
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap();
    let row = save_typed(
        &mut library,
        &InferenceModelRow { model, template },
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap();
    drop(library);
    row
}

/// Explicit alternate checkpoint for the bounded claim-support gate. The
/// ordinary SmolLM2 fixtures retain their original artifact and template pins.
#[cfg(feature = "inference-gpu")]
async fn save_cross_source_answer_model(data_root: &std::path::Path) -> eidetic::ManifestId {
    let Some(directory) = std::env::var_os("DJINN_CROSS_SOURCE_QWEN_DIR") else {
        return save_pinned_answer_model(data_root).await;
    };
    use distillery::inference::{InferenceModelRow, PromptTemplate};
    use eidetic::{PrivacyClass, ProvenanceRecord, Timestamp, TrustEnvelope};
    use sha2::{Digest, Sha256};
    assert!(
        std::env::var_os("CROSS_SOURCE_CASES").is_some()
            || std::env::var_os("DJINN_HEADED_MIXED_QUERIES").is_some()
    );
    let directory = std::path::PathBuf::from(directory);
    let qwen3 = std::env::var_os("DJINN_CROSS_SOURCE_QWEN3").is_some();
    let qwen15 = std::env::var_os("DJINN_CROSS_SOURCE_QWEN15").is_some();
    let reference: serde_json::Value = serde_json::from_slice(if qwen15 {
        include_bytes!("fixtures/esp/qwen2_5_1_5b_instruct_artifacts.json").as_slice()
    } else if qwen3 {
        include_bytes!("fixtures/esp/qwen3_0_6b_artifacts.json").as_slice()
    } else {
        include_bytes!("fixtures/esp/qwen2_5_0_5b_instruct_artifacts.json").as_slice()
    })
    .unwrap();
    let read = |name: &str| {
        let bytes = std::fs::read(directory.join(name)).unwrap();
        let hash: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hash,
            reference["artifacts"][name].as_str().unwrap(),
            "exact {name}"
        );
        bytes
    };
    let path = djinn::resident_distillery::model_library_path(data_root, &profile().0, None);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut library = muniment::RedbBackend::open(path).unwrap();
    let model = if qwen15 {
        // The established layer-streaming loader bounds tensor residence;
        // this keeps the original f32 execution semantics and source bytes.
        use std::io::Read;
        for (name, hash) in reference["artifacts"].as_object().unwrap() {
            let mut file = std::fs::File::open(directory.join(name)).unwrap();
            let mut digest = Sha256::new();
            let mut buffer = vec![0; 1024 * 1024];
            loop {
                let n = file.read(&mut buffer).unwrap();
                if n == 0 {
                    break;
                }
                digest.update(&buffer[..n]);
            }
            let actual: String = digest
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(actual, hash.as_str().unwrap(), "exact {name}");
        }
        esp::infer::layer_library::import_layered_model(
            &mut library,
            &directory,
            reference["model_id"].as_str().unwrap(),
            "Apache-2.0",
            esp::infer::decoder::streaming::StreamingOptions::default(),
        )
        .await
        .unwrap()
    } else {
        let config = read("config.json");
        let tokenizer = read("tokenizer.json");
        let weights = read("model.safetensors");
        read("tokenizer_config.json");
        let imported = eidetic::ModelLibrary::save_model_with_components(
            &mut library,
            reference["model_id"].as_str().unwrap(),
            reference["architecture"].as_str().unwrap(),
            "Apache-2.0",
            serde_json::from_slice(&config).unwrap(),
            &weights,
            &tokenizer,
            vec![],
            vec![],
            PrivacyClass::LocalOnly,
            ProvenanceRecord::self_imported("local"),
            TrustEnvelope::self_asserted(),
            Timestamp::ZERO,
        )
        .await
        .unwrap();
        drop(weights);
        imported
    };
    let template = PromptTemplate::from_bytes(if qwen3 {
        include_bytes!("fixtures/esp/qwen3_non_thinking_v1.json").as_slice()
    } else {
        include_bytes!("fixtures/esp/qwen2_5_instruct_v1.json").as_slice()
    })
    .unwrap();
    let template = eidetic::typed::save_typed(
        &mut library,
        &template,
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap();
    eidetic::typed::save_typed(
        &mut library,
        &InferenceModelRow { model, template },
        vec![],
        PrivacyClass::LocalOnly,
        ProvenanceRecord::self_imported("local"),
        TrustEnvelope::self_asserted(),
        Timestamp::ZERO,
    )
    .await
    .unwrap()
}

/// Test-owned native resident for the separate, real headed Turnstone process.
/// Control files contain public fixture state only; vault material stays temporary.
#[cfg(feature = "inference-gpu")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires ESP_SMOLLM2_DIR, DJINN_HEADED_DIR and headed Turnstone"]
async fn serve_trained_model_for_headed_turnstone() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("distillery::inference::gpu=debug")
        .with_ansi(false)
        .try_init();
    let output =
        std::path::PathBuf::from(std::env::var_os("DJINN_HEADED_DIR").expect("DJINN_HEADED_DIR"));
    std::fs::create_dir_all(&output).unwrap();
    // The default remains the original in-process two-phase receipt. An
    // explicit phase and private root let a consumer terminate this process,
    // read offline, and reopen the same custody/library/journal in a new one.
    let phase = std::env::var("DJINN_HEADED_PHASE").ok().map(|value| {
        let phase: usize = value.parse().expect("phase is 0 or 1");
        assert!(phase <= 1);
        phase
    });
    let temp = tempfile::tempdir().unwrap();
    let private_root = std::env::var_os("DJINN_HEADED_ROOT").map(std::path::PathBuf::from);
    assert!(
        phase.is_none() || private_root.is_some(),
        "a process phase needs an explicit private root"
    );
    let root = private_root.as_deref().unwrap_or(temp.path());
    let data_root = root.join("data");
    let (vault_dir, identity) = open_vault(root);
    write_device_settings(&data_root, Some(lending()));
    let row_file = output.join("model-row.json");
    let row = if phase == Some(1) {
        serde_json::from_slice(&std::fs::read(&row_file).unwrap()).unwrap()
    } else {
        let row = save_cross_source_answer_model(&data_root).await;
        std::fs::write(&row_file, serde_json::to_vec(&row).unwrap()).unwrap();
        row
    };
    // Explicit mixed-source qualification. The ordinary single-source fixture
    // remains unchanged; this file supplies questions, never source bodies.
    let mixed: Option<serde_json::Value> = std::env::var_os("DJINN_HEADED_MIXED_QUERIES")
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap());
    if let Some(mixed) = &mixed {
        assert!(phase.is_some() && private_root.is_some());
        let asks = mixed["asks"].as_array().unwrap();
        assert!((1..=8).contains(&asks.len()));
        let mut ids = std::collections::BTreeSet::new();
        for case in asks {
            let id = case["id"].as_str().unwrap();
            assert!(
                !id.is_empty()
                    && id.len() <= 48
                    && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            );
            assert!(ids.insert(id));
            assert!(!case["question"].as_str().unwrap().is_empty());
        }
    }
    // Optional embedding qualification uses the same ordinary headed fixture,
    // source publication and private journal as trained lexical recall.
    let embedding = std::env::var_os("DJINN_HEADED_EMBEDDING_DIR").map(|directory| {
        assert!(cfg!(feature = "inference-embedding"));
        assert!(
            private_root.is_some(),
            "embedding gate requires a retained root"
        );
        let file = output.join("embedding-model.json");
        let model: eidetic::ManifestId = if phase == Some(1) {
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap()
        } else {
            let directory = std::path::PathBuf::from(directory);
            let path =
                djinn::resident_distillery::model_library_path(&data_root, &profile().0, None);
            let mut store = muniment::RedbBackend::open(path).unwrap();
            let model = pollster::block_on(eidetic::ModelLibrary::save_model_with_components(
                &mut store,
                "sentence-transformers/all-MiniLM-L6-v2",
                "bert",
                "Apache-2.0",
                serde_json::from_slice(&std::fs::read(directory.join("config.json")).unwrap())
                    .unwrap(),
                &std::fs::read(directory.join("model.safetensors")).unwrap(),
                &std::fs::read(directory.join("tokenizer.json")).unwrap(),
                vec![],
                vec![],
                eidetic::PrivacyClass::LocalOnly,
                eidetic::ProvenanceRecord::self_imported(
                    "headed retained-source embedding qualification",
                ),
                eidetic::TrustEnvelope::self_asserted(),
                eidetic::Timestamp::ZERO,
            ))
            .unwrap();
            std::fs::write(&file, serde_json::to_vec(&model).unwrap()).unwrap();
            model
        };
        distillery::inference::recall::EmbeddingRecallSettings {
            provider: distillery::inference::recall::EmbeddingRecallProvider::Library {
                model: model.to_string(),
            },
            max_index_bytes: 8 * 1024 * 1024,
            lexical_weight: 1,
            vector_weight: 1,
            rrf_k: 60,
        }
    });
    let settings = OwnerSettings {
        inference: Some(InferenceLaneSettings {
            embedding: embedding.clone(),
            recall: distillery::inference::recall::RecallConfig {
                max_passages: if mixed.as_ref().is_some_and(|v| v["claim_support"] == true) {
                    4
                } else if mixed.is_some() {
                    3
                } else {
                    1
                },
                ..Default::default()
            },
            model: InferenceModelSettings::Library {
                weight_cache: std::env::var_os("DJINN_CROSS_SOURCE_QWEN15").map(|_| {
                    djinn::settings::InferenceWeightCacheSettings {
                        host_bytes: 3 * 1024_u64.pow(3),
                        device_layer_bytes: 4 * 1024_u64.pow(3),
                    }
                }),
                row: row.to_string(),
                device: djinn::settings::InferenceDeviceSettings::Gpu,
                max_tokens: if mixed.as_ref().is_some_and(|v| v["claim_support"] == true) {
                    1024
                } else {
                    512
                },
            },
        }),
        ..owner(Some(lane()))
    };
    // Four independently bounded asks plus native inspection and shutdown.
    let headed_seconds = if std::env::var_os("DJINN_CROSS_SOURCE_QWEN15").is_some() {
        4200
    } else {
        1200
    };
    let endpoint_file = output.join("endpoint.txt");
    let endpoint = std::fs::read_to_string(&endpoint_file).unwrap_or_else(|_| endpoint_name());
    std::fs::write(endpoint_file, &endpoint).unwrap();
    let mut snapshots: Vec<serde_json::Value> = vec![];
    if phase == Some(1) {
        snapshots.push(
            serde_json::from_slice(&std::fs::read(output.join("snapshot-0.json")).unwrap())
                .unwrap(),
        );
    }
    let mut saw_stream = output.join("streaming.json").exists();
    let mut saw_pause = output.join("paused.json").exists();
    for phase in phase.map(|phase| phase..phase + 1).unwrap_or(0..2) {
        let resident = DjinnResident::open(
            &identity,
            &data_root,
            &profile(),
            settings.clone(),
            &vault_dir,
            unlock(),
        )
        .await
        .unwrap();
        let lane = resident.inference().unwrap().lane();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        while lane.runs().session.is_none() {
            assert!(lane.runs().load_error.is_none(), "{:?}", lane.runs());
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let catalog = AppEndpointCatalog::new(ResidentEndpointCatalog::new());
        let grants = AppRouteGrants::new(AllowedAppRoutes::new([]));
        let route = catalog
            .update(|catalog| resident.register_inference_route(catalog))
            .await
            .unwrap()
            .unwrap();
        grants.grant(AppId::new("turnstone"), route);
        // Opt-in receiving fixture: the same broker supplies the normal
        // wallet-backed clip custody route alongside inference. Both cold
        // processes use this marked wallet; no host-side fallback key exists.
        let keeper = Arc::new(djinn::keeper::Keeper::new(keeper_host()));
        if let Some(root) = std::env::var_os("DJINN_HEADED_CLIP_CUSTODY") {
            let root = std::path::PathBuf::from(root);
            assert_eq!(
                std::fs::read_to_string(root.join("fixture-marker")).unwrap(),
                "retained-embedding-20261020\n"
            );
            let persona = pandect::PersonaId::from_uuid(uuid::Uuid::from_u128(0xe1de_2026_1020));
            castellan::custody::wallet::ensure_wallet_state(
                &root,
                persona,
                "Retained recall fixture",
            )
            .unwrap();
            keeper
                .configure_payload_custody(personae::ProfileId("default".into()), persona, root)
                .unwrap();
            djinn::custody::grant(&grants).unwrap();
        }
        let server = {
            let endpoint = endpoint.clone();
            let grants = grants.clone();
            let catalog = catalog.clone();
            tokio::spawn(async move {
                let _ = serve_app_broker(
                    &endpoint,
                    keeper,
                    grants,
                    headed_seconds * 1000,
                    None,
                    catalog,
                )
                .await;
            })
        };
        // Prove the application endpoint is accepting before announcing readiness.
        for attempt in 0..100 {
            if let Ok(mut client) = open(&endpoint, "turnstone").await {
                let _ = client.close().await;
                break;
            }
            assert!(attempt < 99);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        std::fs::write(
            output.join(format!("ready-{phase}.json")),
            serde_json::to_vec_pretty(&lane.runs()).unwrap(),
        )
        .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(headed_seconds);
        let mut last_observation = Vec::new();
        loop {
            let runs = lane.runs();
            let observation = serde_json::to_vec_pretty(&runs).unwrap();
            if observation != last_observation {
                std::fs::write(output.join(format!("live-runs-{phase}.json")), &observation)
                    .unwrap();
                last_observation = observation;
            }
            if runs.runs.len() >= 2
                && runs
                    .runs
                    .iter()
                    .skip(1)
                    .any(|run| matches!(run.state, RunStateV1::Queued | RunStateV1::Running))
            {
                std::fs::write(output.join("second-started.json"), b"started").unwrap();
            }
            for run in &runs.runs {
                if let Some(mixed) = &mixed {
                    if run.state == RunStateV1::Finished {
                        let recall = run.recall.as_ref().unwrap();
                        if mixed["claim_support"] != true {
                            assert_eq!(
                                recall.scope.len(),
                                4,
                                "mixed scope remains explicit and complete"
                            );
                        }
                        for case in mixed["asks"].as_array().unwrap() {
                            if case["question"].as_str().unwrap() == run.question {
                                let historical = phase == 1
                                    && snapshots[0]["runs"]["runs"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .any(|old| old["run"] == run.run);
                                let name = if historical {
                                    format!("history-1-{}.json", case["id"].as_str().unwrap())
                                } else {
                                    format!("mixed-{phase}-{}.json", case["id"].as_str().unwrap())
                                };
                                if !output.join(&name).exists()
                                    && (!historical
                                        || if mixed["claim_support"] == true {
                                            // Wait for retained source publication before
                                            // freezing the historical live observation.
                                            recall.stale.is_empty()
                                                && recall.unknown_citations.is_empty()
                                        } else {
                                            !recall.citations.is_empty()
                                        })
                                {
                                    std::fs::write(
                                        output.join(name),
                                        serde_json::to_vec_pretty(run).unwrap(),
                                    )
                                    .unwrap();
                                }
                            }
                        }
                        let file = output.join(format!("mixed-rankings-{phase}.json"));
                        if !file.exists() && !recall.citations.is_empty() {
                            let owner = recall.owner.as_deref().unwrap();
                            let mut rows = Vec::new();
                            for case in mixed["rankings"].as_array().unwrap() {
                                let question = case["question"].as_str().unwrap();
                                let ranked = lane
                                    .published_recall_rankings(owner, question, &recall.scope)
                                    .unwrap();
                                rows.push(serde_json::json!({"id":case["id"],"question":question,
                                    "lexical":ranked.lexical,"vector":ranked.vector,"fused":ranked.fused}));
                            }
                            std::fs::write(file, serde_json::to_vec_pretty(&serde_json::json!({
                                "owner":owner,"scope":recall.scope,"config":recall.config,"rows":rows,
                                "boundary":"actual live published binding; same scoped current-version/provider path as generation"
                            })).unwrap()).unwrap();
                        }
                    }
                }
                if run.state == RunStateV1::Running && run.tokens > 0 {
                    saw_stream = true;
                    std::fs::write(
                        output.join("streaming.json"),
                        serde_json::to_vec_pretty(run).unwrap(),
                    )
                    .unwrap();
                }
                if run.state == RunStateV1::Paused {
                    saw_pause = true;
                    std::fs::write(
                        output.join("paused.json"),
                        serde_json::to_vec_pretty(run).unwrap(),
                    )
                    .unwrap();
                }
                if run.state == RunStateV1::Finished && !output.join("finished.json").exists() {
                    std::fs::write(
                        output.join("finished.json"),
                        serde_json::to_vec_pretty(run).unwrap(),
                    )
                    .unwrap();
                }
                if phase == 1
                    && run.state == RunStateV1::Finished
                    && run
                        .recall
                        .as_ref()
                        .is_some_and(|recall| !recall.citations.is_empty())
                {
                    std::fs::write(
                        output.join("reopened.json"),
                        serde_json::to_vec_pretty(run).unwrap(),
                    )
                    .unwrap();
                }
                if phase == 1
                    && !output.join("embedding-cold-ask.json").exists()
                    && run.state == RunStateV1::Finished
                    && run.recall.as_ref().is_some_and(|r| {
                        !r.citations.is_empty()
                            && r.embedding.as_ref().is_some_and(|e| e.cache == "reopened")
                    })
                {
                    std::fs::write(
                        output.join("embedding-cold-ask.json"),
                        serde_json::to_vec_pretty(run).unwrap(),
                    )
                    .unwrap();
                }
                if run.state == RunStateV1::Cancelled && !output.join("cancelled.json").exists() {
                    std::fs::write(
                        output.join("cancelled.json"),
                        serde_json::to_vec_pretty(run).unwrap(),
                    )
                    .unwrap();
                }
            }
            if output
                .join(if phase == 0 {
                    "restart.request"
                } else {
                    "stop.request"
                })
                .exists()
            {
                let journal = resident.inference().unwrap().journal().unwrap();
                let mut evidence = vec![];
                for run in &runs.runs {
                    let record = journal.retained_record(&run.run).unwrap();
                    let passages: Vec<_> = record
                        .supplied_passages
                        .iter()
                        .map(|id| journal.resolve_passage(*id).unwrap().unwrap())
                        .collect();
                    let checkpoint = record.checkpoint_blob.map(|id| {
                        serde_json::from_slice::<serde_json::Value>(
                            &journal.resolve_evidence_blob(id).unwrap().unwrap(),
                        )
                        .unwrap()
                    });
                    let support = journal.retained_answer_support(&run.run).unwrap();
                    let requested_facts = journal.retained_requested_facts(&run.run).unwrap();
                    if let Some(expected) = run.recall.as_ref().and_then(|r| r.support.as_ref()) {
                        assert_eq!(support.as_ref(), Some(expected));
                    }
                    evidence.push(serde_json::json!({"record":record,"passages":passages,"checkpoint":checkpoint,"answer_support":support,"requested_facts":requested_facts}));
                }
                // Store enumeration order may change across OS processes.
                // Compare every sealed field under the stable run identity.
                evidence.sort_by(|a, b| {
                    a["record"]["run"]
                        .as_str()
                        .unwrap()
                        .cmp(b["record"]["run"].as_str().unwrap())
                });
                let projections = std::collections::BTreeMap::<String, serde_json::Value>::new();
                #[cfg(feature = "inference-embedding")]
                let mut projections = projections;
                #[cfg(feature = "inference-embedding")]
                for run in &runs.runs {
                    if let Some(e) = run.recall.as_ref().and_then(|r| r.embedding.as_ref()) {
                        let bytes = journal
                            .load_embedding_projection(e.envelope, 8 * 1024 * 1024)
                            .unwrap()
                            .unwrap();
                        assert_eq!(eidetic::ManifestId::of_blob(&bytes), e.index);
                        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                        assert_eq!(value["envelope"]["space"]["dimensions"], 384);
                        projections.insert(e.envelope.to_string(), serde_json::json!({"index":e.index,"envelope":value["envelope"],"persisted_bytes":bytes.len()}));
                    }
                }
                snapshots.push(serde_json::json!({"phase":phase,"runs":runs,"evidence":evidence,"embedding_projections":projections}));
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "headed fixture timed out"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        server.abort();
        let _ = server.await;
        drop(catalog);
        resident.shutdown().await.unwrap();
        std::fs::write(
            output.join(format!("snapshot-{phase}.json")),
            serde_json::to_vec_pretty(snapshots.last().unwrap()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            output.join(format!("process-{phase}.json")),
            serde_json::to_vec_pretty(
                &serde_json::json!({"pid":std::process::id(),"phase":phase,"clean_shutdown":true}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    if phase == Some(0) {
        return;
    }
    let receipt = serde_json::json!({"model_row":row,"streaming_observed":saw_stream,"pause_observed":saw_pause,"snapshots":snapshots,"boundary":"real GPU model in test-owned native Djinn resident, separate headed Turnstone process over actual app connection"});
    std::fs::write(
        output.join("resident-receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    if let Some(mixed) = &mixed {
        if mixed["claim_support"] == true {
            // This scenario reopens every historical answer and generates one
            // fresh cross-source answer. Insufficient answers correctly have
            // no citations; the independent frozen rubric checks semantics.
            assert_eq!(snapshots.len(), 2);
            let load = |name: String| -> serde_json::Value {
                serde_json::from_slice(&std::fs::read(output.join(name)).unwrap()).unwrap()
            };
            for case in mixed["asks"].as_array().unwrap() {
                let id = case["id"].as_str().unwrap();
                let warm = load(format!("mixed-0-{id}.json"));
                assert_eq!(warm, load(format!("history-1-{id}.json")));
                let retained = |snapshot: &serde_json::Value| {
                    snapshot["evidence"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|e| e["record"]["run"] == warm["run"])
                        .unwrap()
                        .clone()
                };
                assert_eq!(retained(&snapshots[0]), retained(&snapshots[1]));
            }
            let warm = load("mixed-0-cross.json".into());
            let fresh = load("mixed-1-cross.json".into());
            assert_eq!(fresh["state"], "finished");
            assert_ne!(warm["run"], fresh["run"]);
            assert_eq!(warm["question"], fresh["question"]);
            for field in ["scope", "passages"] {
                assert_eq!(warm["recall"][field], fresh["recall"][field]);
            }
            if embedding.is_some() {
                assert_eq!(warm["recall"]["embedding"]["cache"], "built");
                assert_eq!(fresh["recall"]["embedding"]["cache"], "reopened");
                for field in ["envelope", "index"] {
                    assert_eq!(
                        warm["recall"]["embedding"][field],
                        fresh["recall"]["embedding"][field]
                    );
                }
                assert_eq!(
                    snapshots[0]["embedding_projections"],
                    snapshots[1]["embedding_projections"]
                );
            }
            return;
        }
        for case in mixed["asks"].as_array().unwrap() {
            let id = case["id"].as_str().unwrap();
            let load = |name: String| -> serde_json::Value {
                serde_json::from_slice(&std::fs::read(output.join(name)).unwrap()).unwrap()
            };
            let warm = load(format!("mixed-0-{id}.json"));
            let cold = load(format!("mixed-1-{id}.json"));
            let history = load(format!("history-1-{id}.json"));
            assert_eq!(
                warm, history,
                "live historical source/citation state is restored"
            );
            assert_ne!(warm["run"], cold["run"]);
            assert_eq!(warm["recall"]["scope"], cold["recall"]["scope"]);
            assert_eq!(warm["recall"]["passages"], cold["recall"]["passages"]);
            for run in [&warm, &cold] {
                assert_eq!(run["state"], "finished");
                assert!(!run["recall"]["citations"].as_array().unwrap().is_empty());
                assert!(run["recall"]["stale"].as_array().unwrap().is_empty());
            }
            if embedding.is_some() {
                assert_eq!(warm["recall"]["embedding"]["cache"], "built");
                assert_eq!(cold["recall"]["embedding"]["cache"], "reopened");
                for field in ["envelope", "index"] {
                    assert_eq!(
                        warm["recall"]["embedding"][field],
                        cold["recall"]["embedding"][field]
                    );
                }
            } else {
                assert!(
                    warm["recall"]["embedding"].is_null() && cold["recall"]["embedding"].is_null()
                );
            }
            let old = snapshots[0]["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["record"]["run"] == warm["run"])
                .unwrap();
            let restored = snapshots[1]["evidence"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["record"]["run"] == warm["run"])
                .unwrap();
            assert_eq!(
                old, restored,
                "sealed historical record/passages/checkpoint remain immutable"
            );
        }
        assert_eq!(
            snapshots[0]["embedding_projections"],
            snapshots[1]["embedding_projections"]
        );
        assert_eq!(
            std::fs::read(output.join("mixed-rankings-0.json")).unwrap(),
            std::fs::read(output.join("mixed-rankings-1.json")).unwrap()
        );
        return;
    }
    if embedding.is_some() {
        let first = snapshots[0]["runs"]["runs"].as_array().unwrap();
        let warm = first.iter().find(|r| r["state"] == "finished").unwrap();
        let cold = snapshots[1]["runs"]["runs"].as_array().unwrap();
        let restored = cold.iter().find(|r| r["run"] == warm["run"]).unwrap();
        assert_eq!(
            restored, warm,
            "cold reopen preserves the historical answer and exact citations"
        );
        let new = cold
            .iter()
            .find(|r| r["state"] == "finished" && r["run"] != warm["run"])
            .unwrap();
        assert_eq!(warm["recall"]["embedding"]["cache"], "built");
        assert_eq!(new["recall"]["embedding"]["cache"], "reopened");
        for field in ["envelope", "index"] {
            assert_eq!(
                new["recall"]["embedding"][field],
                warm["recall"]["embedding"][field]
            );
        }
        assert_eq!(new["recall"]["passages"], warm["recall"]["passages"]);
        // The app has exited before these snapshots: its publisher withdraws
        // live heads. Validate citation admission from the live observations,
        // while comparing the immutable historical evidence below separately.
        let live_warm: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("finished.json")).unwrap()).unwrap();
        let live_cold: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("embedding-cold-ask.json")).unwrap())
                .unwrap();
        assert!(
            !live_warm["recall"]["citations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            !live_cold["recall"]["citations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            live_warm["recall"]["passages"],
            live_cold["recall"]["passages"]
        );
        assert_eq!(
            snapshots[0]["embedding_projections"],
            snapshots[1]["embedding_projections"]
        );
        let old = snapshots[0]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["record"]["run"] == warm["run"])
            .unwrap();
        let restored = snapshots[1]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["record"]["run"] == warm["run"])
            .unwrap();
        assert_eq!(
            old, restored,
            "historical sealed source and prompt evidence is immutable"
        );
        return;
    }
    assert!(saw_stream && saw_pause);
    let first = &snapshots[0]["runs"]["runs"];
    let finished = first
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["state"] == "finished")
        .unwrap();
    let paused: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("paused.json")).unwrap()).unwrap();
    assert!(paused["tokens"].as_u64().unwrap() > 0);
    assert!(
        finished["text"]
            .as_str()
            .unwrap()
            .starts_with(paused["text"].as_str().unwrap())
    );
    assert!(
        finished["continuations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|continuation| continuation["reason"] == "user"
                && continuation["at_token"].as_u64().unwrap() > 0)
    );
    let reopened = snapshots[1]["runs"]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["run"] == finished["run"])
        .unwrap();
    assert_eq!(
        reopened, finished,
        "reopened published source must restore the same answer and citations"
    );
    let live_finished: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("finished.json")).unwrap()).unwrap();
    let live_reopened: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("reopened.json")).unwrap()).unwrap();
    assert!(
        !live_finished["recall"]["citations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        live_reopened, live_finished,
        "live source publication restores exact citations after restart"
    );
    assert_eq!(
        snapshots[0]["evidence"], snapshots[1]["evidence"],
        "reopening cannot rewrite sealed source/prompt evidence"
    );
    assert!(
        first
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["state"] == "finished")
    );
    assert!(
        first
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["state"] == "cancelled")
    );
}

/// Provider capability probe only. The headed gate separately admits real
/// captures and producer packages through their ordinary custody owners.
#[cfg(feature = "inference-gpu")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires pinned ESP_SMOLLM2_DIR and CROSS_SOURCE_CASES/OUTPUT"]
async fn probe_bounded_cross_source_answers() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("distillery::inference::gpu=debug")
        .with_ansi(false)
        .try_init();
    use distillery::inference::recall::{
        AnswerFormat, PageTextCorpus, RecallAskRequestV1, RecallConfig,
    };
    use eidetic::models::recall::{RecallSourceId, RecallSourceKind};
    let cases: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("CROSS_SOURCE_CASES").unwrap()).unwrap(),
    )
    .unwrap();
    let output = std::path::PathBuf::from(std::env::var_os("CROSS_SOURCE_OUTPUT").unwrap());
    std::fs::create_dir_all(&output).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let (vault_dir, identity) = open_vault(temp.path());
    let data_root = temp.path().join("data");
    write_device_settings(&data_root, Some(lending()));
    let row = save_cross_source_answer_model(&data_root).await;
    let config = RecallConfig {
        max_passages: 4,
        ..Default::default()
    };
    let settings = OwnerSettings {
        inference: Some(InferenceLaneSettings {
            embedding: None,
            recall: config.clone(),
            model: InferenceModelSettings::Library {
                weight_cache: std::env::var_os("DJINN_CROSS_SOURCE_QWEN15").map(|_| {
                    djinn::settings::InferenceWeightCacheSettings {
                        host_bytes: 3 * 1024_u64.pow(3),
                        device_layer_bytes: 4 * 1024_u64.pow(3),
                    }
                }),
                row: row.to_string(),
                device: djinn::settings::InferenceDeviceSettings::Gpu,
                max_tokens: 1024,
            },
        }),
        ..owner(Some(lane()))
    };
    let resident = DjinnResident::open(
        &identity,
        &data_root,
        &profile(),
        settings,
        &vault_dir,
        unlock(),
    )
    .await
    .unwrap();
    let store = Arc::new(muniment::RedbBackend::open(temp.path().join("source.redb")).unwrap());
    for (key, value) in cases["corpus"].as_object().unwrap() {
        eidetic::PageTextStore::new(store.as_ref())
            .put(
                &format!("https://probe.test/{key}"),
                value["text"].as_str().unwrap(),
                1,
            )
            .await
            .unwrap();
    }
    resident
        .configure_inference_recall(Arc::new(PageTextCorpus(store)), config)
        .unwrap();
    let lane = resident.inference().unwrap().lane();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    while lane.runs().session.is_none() {
        assert!(lane.runs().load_error.is_none(), "{:?}", lane.runs());
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut incomplete = Vec::new();
    for case in cases["asks"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let keys: &[&str] = match id {
            "conflict" => &["capture", "control", "knot", "note"],
            "reduced" => &["capture", "knot"],
            _ => &["capture", "knot", "note"],
        };
        lane.ask_recalled(RecallAskRequestV1 {
            answer_format: AnswerFormat::ClaimSupportV1,
            ask: AskRequestV1 {
                run: id.into(),
                question: case["question"].as_str().unwrap().into(),
            },
            scope: keys
                .iter()
                .map(|key| RecallSourceId {
                    kind: RecallSourceKind::PageText,
                    source: format!("https://probe.test/{key}"),
                    owner: None,
                    node: None,
                })
                .collect(),
        })
        .unwrap();
        let seconds = if std::env::var_os("DJINN_CROSS_SOURCE_QWEN15").is_some() {
            900
        } else {
            300
        };
        let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
        let mut cancel_deadline = None;
        loop {
            let runs = lane.runs();
            let run = runs.runs.iter().find(|r| r.run == id).unwrap();
            std::fs::write(
                output.join(format!("{id}.partial.json")),
                serde_json::to_vec_pretty(run).unwrap(),
            )
            .unwrap();
            if run.state.is_terminal() {
                std::fs::write(
                    output.join(format!("{id}.json")),
                    serde_json::to_vec_pretty(run).unwrap(),
                )
                .unwrap();
                if run.state != RunStateV1::Finished || cancel_deadline.is_some() {
                    incomplete.push((id.to_owned(), run.state));
                }
                break;
            }
            let now = tokio::time::Instant::now();
            if let Some(until) = cancel_deadline {
                assert!(
                    now < until,
                    "timed-out case did not cancel within 30 seconds"
                );
            } else if now >= deadline {
                std::fs::write(output.join(format!("{id}.timeout.json")), serde_json::to_vec_pretty(&serde_json::json!({"limit_seconds":seconds,"last_state":run.state,"last_tokens":run.tokens,"reason":"generation_deadline"})).unwrap()).unwrap();
                lane.cancel(&CancelRequestV1 { run: id.into() }).unwrap();
                cancel_deadline = Some(now + Duration::from_secs(30));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    resident.shutdown().await.unwrap();
    assert!(
        incomplete.is_empty(),
        "incomplete frozen cases: {incomplete:?}"
    );
}
