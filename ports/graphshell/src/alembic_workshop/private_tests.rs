// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Real authenticated cipher and real directory/GraphSession writes. Process
//! custody and headed/native input are independent receiving gates.
use super::*;
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use muniment::Backend;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const CANARY: &str = "private-graphshell-canary · café 🦉 not for logs";
struct Cipher {
    epoch: AtomicU64,
    key: [u8; 32],
}
impl Cipher {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            epoch: AtomicU64::new(1),
            key: [0x82; 32],
        })
    }
}
impl Protection for Cipher {
    fn epoch(&self) -> Option<u64> {
        let n = self.epoch.load(Ordering::SeqCst);
        (n != 0).then_some(n)
    }
    fn seal(&self, context: &[u8], clear: &[u8]) -> Result<Vec<u8>, workshop::Error> {
        self.epoch().ok_or(workshop::Error::Unauthorized)?;
        let random = Uuid::new_v4();
        let nonce = &random.as_bytes()[..12];
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key).unwrap();
        let bytes = cipher
            .encrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: clear,
                    aad: context,
                },
            )
            .map_err(|_| workshop::Error::Conflict)?;
        let mut sealed = nonce.to_vec();
        sealed.extend(bytes);
        Ok(sealed)
    }
    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, workshop::Error> {
        self.epoch().ok_or(workshop::Error::Unauthorized)?;
        if sealed.len() < 12 {
            return Err(workshop::Error::Conflict);
        }
        ChaCha20Poly1305::new_from_slice(&self.key)
            .unwrap()
            .decrypt(
                Nonce::from_slice(&sealed[..12]),
                Payload {
                    msg: &sealed[12..],
                    aad: context,
                },
            )
            .map_err(|_| workshop::Error::Conflict)
    }
}
fn open(root: &Path, cipher: Arc<Cipher>) -> GraphshellWorkshop {
    pollster::block_on(GraphshellWorkshop::open_sealed(
        root,
        Arc::new(|| {}),
        cipher,
        [9; 32],
        "synthetic".into(),
    ))
    .unwrap()
}
fn retained(owner: &mut GraphshellWorkshop) {
    for i in 1..=3 {
        let text = format!("{CANARY} / {i}");
        pollster::block_on(owner.retain_private_source(
            Uuid::from_u128(i),
            format!("Secret source {i}"),
            text.clone(),
            0,
            text.len() as u64,
        ))
        .unwrap();
    }
}
fn generated(owner: &mut GraphshellWorkshop) -> SurfaceState {
    let mut state = SurfaceState::new();
    pollster::block_on(owner.refresh_surface(&mut state)).unwrap();
    assert_eq!(state.choices.len(), 3);
    for c in &mut state.choices {
        c.selected = true;
    }
    state.request(SurfaceAction::Run);
    owner.service(&mut state);
    let deadline = Instant::now() + Duration::from_secs(10);
    while owner.busy() && Instant::now() < deadline {
        owner.service(&mut state);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!owner.busy());
    assert!(state.error.is_none(), "{:?}", state.error);
    assert_eq!(state.view.as_ref().unwrap().phase, RunStatus::Completed);
    state
}
fn no_clear(path: &Path) {
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            no_clear(&entry.path());
        } else {
            let bytes = std::fs::read(entry.path()).unwrap();
            for needle in [CANARY, "Secret source"] {
                assert!(
                    !bytes.windows(needle.len()).any(|w| w == needle.as_bytes()),
                    "private bytes in {}",
                    entry.path().display()
                );
            }
        }
    }
}
#[test]
fn sealed_graphshell_cold_reopen_exact_citations_and_helper_independent_result() {
    let dir = tempfile::tempdir().unwrap();
    let cipher = Cipher::new();
    let mut owner = open(dir.path(), cipher.clone());
    retained(&mut owner);
    let mut state = generated(&mut owner);
    let proposal = state.view.as_ref().unwrap().proposal.clone().unwrap();
    assert!(proposal.sources.iter().all(|p| !p.public));
    assert_eq!(
        proposal
            .sources
            .iter()
            .map(|p| p.source.title.as_str())
            .collect::<Vec<_>>(),
        ["Secret source 1", "Secret source 2", "Secret source 3"]
    );
    state.request(SurfaceAction::OpenSource {
        proposal: proposal.digest,
        source: proposal.sources[1].source.clone(),
    });
    owner.service(&mut state);
    assert_eq!(
        state.reading.as_ref().unwrap().text,
        format!("{CANARY} / 2")
    );
    state.request(SurfaceAction::Control(Control::Accept));
    owner.service(&mut state);
    assert!(state.error.is_none(), "{:?}", state.error);
    let Some(Review::Added { receipt, .. }) = &state.view.as_ref().unwrap().review else {
        panic!("no durable receipt")
    };
    let receipt = receipt.clone();
    let identity = owner.owner.clone();
    owner.close().unwrap();
    drop(owner);
    no_clear(dir.path());
    let mut owner = open(dir.path(), cipher.clone());
    let mut state = SurfaceState::new();
    pollster::block_on(owner.refresh_surface(&mut state)).unwrap();
    assert_eq!(owner.owner, identity);
    assert_eq!(
        state.view.as_ref().unwrap().proposal.as_ref(),
        Some(&proposal)
    );
    state.request(SurfaceAction::Control(Control::Dissolve));
    owner.service(&mut state);
    assert!(state.error.is_none(), "{:?}", state.error);
    owner.close().unwrap();
    drop(owner);
    let backend = DirectoryBackend::open(dir.path().join("workshop")).unwrap();
    pollster::block_on(async {
        let keys = backend.list("alembic/workshop/sealed/").await.unwrap();
        assert!(!keys.is_empty(), "no helper journal was removed");
        for key in keys {
            backend.delete(&key).await.unwrap();
        }
    });
    drop(backend);
    let mut owner = open(dir.path(), cipher);
    let mut state = SurfaceState::new();
    pollster::block_on(owner.refresh_surface(&mut state)).unwrap();
    assert!(
        state.view.as_ref().unwrap().proposal.is_none(),
        "helper journal survived deletion"
    );
    state.request(SurfaceAction::OpenResult(receipt.resource.clone()));
    owner.service(&mut state);
    assert!(state.error.is_none(), "{:?}", state.error);
    assert_eq!(state.reading.as_ref().unwrap().text, proposal.djot);
    assert_eq!(owner.graph().node_count(), 4);
    no_clear(dir.path());
}
#[test]
fn sealed_graphshell_loss_clears_presentation_joins_and_never_revives_old_binding() {
    let dir = tempfile::tempdir().unwrap();
    let cipher = Cipher::new();
    let mut owner = open(dir.path(), cipher.clone());
    retained(&mut owner);
    let mut state = generated(&mut owner);
    let proposal = state
        .view
        .as_ref()
        .unwrap()
        .proposal
        .as_ref()
        .unwrap()
        .clone();
    state.request(SurfaceAction::OpenSource {
        proposal: proposal.digest,
        source: proposal.sources[0].source.clone(),
    });
    owner.service(&mut state);
    assert!(state.reading.is_some());
    cipher.epoch.store(0, Ordering::SeqCst);
    assert!(owner.observe_custody(&mut state));
    assert!(state.view.is_none());
    assert!(state.reading.is_none());
    assert!(state.choices.is_empty());
    assert!(!owner.busy());
    cipher.epoch.store(1, Ordering::SeqCst);
    state.request(SurfaceAction::Run);
    owner.service(&mut state);
    assert!(state.view.is_none());
    assert!(owner.capture(&Uuid::from_u128(1).to_string()).is_err());
    owner.close().unwrap();
    drop(owner);
    cipher.epoch.store(2, Ordering::SeqCst);
    let mut fresh = open(dir.path(), cipher);
    assert!(fresh.capture(&Uuid::from_u128(1).to_string()).is_ok());
    fresh.close().unwrap();
    no_clear(dir.path());
}
#[test]
fn sealed_graphshell_lost_ack_reloads_one_operation_without_clear_ledger_request() {
    let dir = tempfile::tempdir().unwrap();
    let cipher = Cipher::new();
    let mut owner = open(dir.path(), cipher.clone());
    retained(&mut owner);
    let state = generated(&mut owner);
    let view = state.view.unwrap();
    let proposed = view.proposal.unwrap();
    let acceptance = pollster::block_on(owner.driver.workshop.accept(
        view.run.unwrap(),
        owner.destination_for(true).unwrap(),
        owner.human,
        &owner.authority,
        now_ms(),
    ))
    .unwrap();
    let receipt = pollster::block_on(owner.host.workshop_commit_sealed(
        &acceptance,
        &proposed,
        owner.human,
        &owner.authority,
        now_ms(),
        cipher.clone(),
    ))
    .unwrap();
    drop(owner);
    no_clear(dir.path());
    let mut reopened = open(dir.path(), cipher);
    let mut state = SurfaceState::new();
    state.request(SurfaceAction::Control(Control::Reconcile));
    reopened.service(&mut state);
    assert!(state.error.is_none(), "{:?}", state.error);
    assert!(
        matches!(&state.view.unwrap().review, Some(Review::Added {receipt: held, ..}) if held == &receipt)
    );
    assert_eq!(reopened.graph().node_count(), 4);
    reopened.close().unwrap();
    no_clear(dir.path());
}
#[test]
fn sealed_graphshell_sources_refuse_wrong_owner_range_collision_and_tamper() {
    let dir = tempfile::tempdir().unwrap();
    let cipher = Cipher::new();
    let mut owner = open(dir.path(), cipher.clone());
    retained(&mut owner);
    let id = Uuid::from_u128(1);
    let original = owner.capture(&id.to_string()).unwrap();
    assert!(
        pollster::block_on(owner.retain_private_source(
            id,
            "replacement".into(),
            "replacement".into(),
            0,
            11
        ))
        .is_err()
    );
    assert_eq!(owner.capture(&id.to_string()).unwrap(), original);
    let mut wrong = owner.owner.clone();
    wrong.session = Uuid::new_v4().to_string();
    assert!(private_sources::capture(owner.graph(), &wrong, id, cipher.as_ref()).is_err());
    assert!(
        private_sources::prepare(
            owner.graph(),
            &owner.owner,
            Uuid::from_u128(50),
            "bad range".into(),
            "café".into(),
            3,
            4,
            cipher.as_ref()
        )
        .is_err()
    );
    let mut snapshot = owner.graph().to_snapshot_at(0);
    let r = snapshot
        .resources
        .iter_mut()
        .find(|r| {
            r.facets
                .iter()
                .any(|f| f.facet == "mere.alembic.retained-private-source/v1")
        })
        .unwrap();
    let mut sealed: Vec<u8> = serde_json::from_str(&r.facets[0].value_json).unwrap();
    let last = sealed.len() - 1;
    sealed[last] ^= 1;
    r.facets[0].value_json = serde_json::to_string(&sealed).unwrap();
    let tampered = mere::kernel::graph::Graph::try_from_recorded_snapshot(&snapshot).unwrap();
    let affected = private_sources::surfaces(&tampered)
        .into_iter()
        .filter(|id| {
            private_sources::capture(&tampered, &owner.owner, *id, cipher.as_ref()).is_err()
        })
        .count();
    assert_eq!(affected, 1);
    owner.close().unwrap();
}

#[test]
fn closed_owner_dto_drops_queued_private_source_actions() {
    let mut state = SurfaceState::new();
    state.request(SurfaceAction::OpenSource {
        proposal: [7; 32],
        source: Source {
            owner: Owner {
                host: "graphshell".into(),
                workspace: "w".into(),
                session: "s".into(),
            },
            resource: "r".into(),
            version: "v".into(),
            content_digest: [1; 32],
            start: 0,
            end: 1,
            title: CANARY.into(),
        },
    });
    let mut closed = SurfaceState::new();
    GraphshellWorkshop::clear_private_surface(&mut closed);
    state.apply_owner_update(closed);
    assert!(
        state.take_actions().is_empty(),
        "closed owner retained a private queued citation"
    );
    assert!(!format!("{state:?}").contains(CANARY));
}

#[test]
fn sealed_graphshell_owner_refuses_operation_evidence_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    let cipher = Cipher::new();
    let mut owner = open(dir.path(), cipher);
    retained(&mut owner);
    let mut state = generated(&mut owner);
    state.request(SurfaceAction::Control(Control::Accept));
    owner.service(&mut state);
    assert!(state.error.is_none(), "{:?}", state.error);
    let before = owner.host.workshop_revision().unwrap();
    let snapshot = owner.graph().to_snapshot_at(0);
    let operation = snapshot
        .resources
        .iter()
        .find(|r| {
            r.facets
                .iter()
                .any(|f| f.facet == "mere.alembic.evidence-brief.sealed-operation/v1")
        })
        .unwrap();
    let resource = chartulary::resource_id_from_canonical_iri(&operation.canonical_iri);
    assert!(
        pollster::block_on(owner.host.workshop_seed(vec![
            GraphDelta::ReplaySetResourceRecordById {
                resource_id: resource,
                record: None
            }
        ]))
        .is_err(),
        "the normal writer removed retained operation evidence"
    );
    assert_eq!(owner.host.workshop_revision().unwrap(), before);
    owner.close().unwrap();
    no_clear(dir.path());
}
