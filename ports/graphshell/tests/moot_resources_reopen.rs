// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Actual mount + signed durable owners. The process proof exits the writer
//! before opening the same databases in a new reader. No GPU or live carrier.

#[path = "../examples/support/moot_resources_fixture.rs"]
mod fixture;
use cambium::{DomHandle, GenetAppRunner, PointerClick};
use fixture::{BOB_FORK_GRANT, BODY, COLLECTION, DESTINATION, Fixture, MOOT, retention};
use gemot::moot::{CollectionId, MootFile, MootId, MootMembershipAction};
use genet_scripted_dom::{NodeId, ScriptedDom};
use graphshell::moot_resources::{Mount, Owners, Scope, View, view};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use moot::resource_coop::Offer;
use muniment::{Backend, RedbBackend};
use notochord::{RetainedAuthority, RevocationLedger};
use personae::{IdentityProvider, InMemoryProvider};
use std::{cell::RefCell, path::Path, rc::Rc};

type Durable = Fixture<RedbBackend>;
type Runner = GenetAppRunner<Mount, fn(&Mount) -> View, View>;
fn runner(mount: Mount) -> Runner {
    let dom: DomHandle = Rc::new(RefCell::new(ScriptedDom::new()));
    GenetAppRunner::new(dom, view as fn(&Mount) -> View, mount)
}
fn find(dom: &ScriptedDom, node: NodeId, id: &str) -> Option<NodeId> {
    if dom.attribute(node, &Namespace::from(""), &LocalName::from("id")) == Some(id) {
        return Some(node);
    }
    dom.dom_children(node)
        .find_map(|child| find(dom, child, id))
}
fn click(runner: &mut Runner, id: &str) {
    let node = find(&runner.dom().borrow(), runner.root(), id).expect("mounted control");
    runner.dispatch_click(node, PointerClick::at((1., 1.)));
}
fn inspect(runner: &mut Runner, id: &str) {
    let reference = runner.state().reading().pane().snapshot().resources[0].reference;
    let hex: String = reference
        .contribution
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    click(runner, &format!("moot-{id}-inspect-{hex}"));
}
fn scope() -> Scope {
    Scope {
        collection: COLLECTION,
        fork_destination: None,
    }
}
async fn create(root: &Path) -> Durable {
    let founder = InMemoryProvider::from_seed([11; 32])
        .master_public_key()
        .to_bytes();
    let moot = MootFile::open(root.join("moot"), MootId(MOOT), founder, retention())
        .await
        .unwrap();
    Fixture::seed(moot, RedbBackend::open(root.join("content.redb")).unwrap()).await
}
async fn disk(root: &Path) -> Durable {
    let moot = MootFile::open_existing(root.join("moot"), MootId(MOOT), retention())
        .await
        .unwrap();
    let rules = moot.snapshot().await.unwrap().governance.rules;
    Fixture {
        alice: InMemoryProvider::from_seed([11; 32]),
        bob: InMemoryProvider::from_seed([12; 32]),
        moot,
        blobs: RedbBackend::open(root.join("content.redb")).unwrap(),
        rules,
    }
}
async fn opened(f: &Durable, reader: &InMemoryProvider, id: &str) -> Runner {
    let mut r = runner(f.mount(id, reader).await);
    inspect(&mut r, id);
    click(&mut r, &format!("moot-{id}-open"));
    r.update(|m| {
        pollster::block_on(f.poll(m, reader, &RevocationLedger::default(), 21)).unwrap();
    });
    assert_eq!(r.state().opened().unwrap().bytes, BODY);
    r
}
async fn reopen(
    f: &Durable,
    reader: &InMemoryProvider,
    authority: RetainedAuthority,
    ledger: &RevocationLedger,
    slots: &RedbBackend,
    key: &str,
) -> Result<Mount, graphshell::moot_resources::Error> {
    Mount::reopen(
        "reopened",
        authority,
        scope(),
        &mut Owners {
            moot: &f.moot,
            identity: reader,
            store: &mut f.blobs.clone(),
            revocations: ledger,
            at_ms: 2_000,
        },
        slots,
        key,
    )
    .await
}

#[test]
fn durable_mount_reopens_in_a_fresh_process_without_replaying_a_queued_fork() {
    let root = tempfile::tempdir().unwrap();
    let module = module_path!()
        .split_once("::")
        .map(|(_, m)| format!("{m}::disk_process_child"))
        .unwrap_or_else(|| "disk_process_child".into());
    let mut pids = vec![];
    for stage in ["seed", "reopen"] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &module, "--ignored", "--nocapture"])
            .env("MERE_MOOT_REOPEN_ROOT", root.path())
            .env("MERE_MOOT_REOPEN_STAGE", stage)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        pids.push(child.id());
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(40);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() > limit {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Owned {stage} process timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{stage}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.path().join(format!("{stage}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(report["pid"], *pids.last().unwrap());
        println!("{report}");
    }
    assert_ne!(pids[0], pids[1]);
}

#[test]
#[ignore = "invoked only by the bounded two-process durable mount proof"]
fn disk_process_child() {
    let root = std::env::var_os("MERE_MOOT_REOPEN_ROOT").expect("owned process root");
    let stage = std::env::var("MERE_MOOT_REOPEN_STAGE").unwrap();
    pollster::block_on(async {
        let root = Path::new(&root);
        let slots = RedbBackend::open(root.join("readings.redb")).unwrap();
        if stage == "seed" {
            let f = create(root).await;
            let a = opened(&f, &f.alice, "alice").await;
            a.state().save(&slots, "alice").await.unwrap();
            let mut b = runner(f.mount("bob", &f.bob).await);
            click(&mut b, "moot-bob-fork-collection");
            assert!(b.state().reading().pane().pending().is_some());
            b.state().save(&slots, "bob").await.unwrap();
            let saved: serde_json::Value =
                serde_json::from_slice(&slots.get("alice").await.unwrap().unwrap()).unwrap();
            assert!(
                saved.get("authority").is_none()
                    && saved.get("bytes").is_none()
                    && saved.get("destination").is_none()
                    && saved.get("pending").is_none()
                    && saved.get("grants").is_none()
            );
        } else {
            assert_eq!(stage, "reopen");
            let f = disk(root).await;
            let before = slots.get("alice").await.unwrap();
            assert!(
                reopen(
                    &f,
                    &f.alice,
                    f.authority(&f.alice, 1_000),
                    &RevocationLedger::default(),
                    &slots,
                    "alice"
                )
                .await
                .unwrap_err()
                .to_string()
                .contains("Expired")
            );
            let a = reopen(
                &f,
                &f.alice,
                f.authority_at(&f.alice, 5_000, 2_000),
                &RevocationLedger::default(),
                &slots,
                "alice",
            )
            .await
            .unwrap();
            assert_eq!(a.opened().unwrap().bytes, BODY);
            assert_eq!(
                a.reading().pane().selected_resource(),
                Some(a.opened().unwrap().reference)
            );
            let b = reopen(
                &f,
                &f.bob,
                f.authority_at(&f.bob, 5_000, 2_000),
                &RevocationLedger::default(),
                &slots,
                "bob",
            )
            .await
            .unwrap();
            assert!(b.opened().is_none() && b.reading().pane().selected_resource().is_none());
            assert!(
                a.reading().pane().pending().is_none() && b.reading().pane().pending().is_none()
            );
            assert!(
                f.moot
                    .authorized_collection(CollectionId(DESTINATION), 2_000)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                slots.get("alice").await.unwrap(),
                before,
                "reopen performs no slot writes"
            );
            assert_eq!(f.blobs.get("blob:notebook").await.unwrap().unwrap(), BODY);
        }
        std::fs::write(root.join(format!("{stage}.json")), serde_json::to_vec_pretty(&serde_json::json!({"stage":stage,"pid":std::process::id(),"signed_redb_owners":true,"carrier":"verified sans-I/O fixture","runtime":"CPU semantic DOM; no GPU"})).unwrap()).unwrap();
    });
}

#[tokio::test]
async fn reopening_uses_current_membership_and_constitution_instead_of_saved_offers() {
    let root = tempfile::tempdir().unwrap();
    let f = create(root.path()).await;
    let slots = RedbBackend::open(root.path().join("readings.redb")).unwrap();
    let r = opened(&f, &f.bob, "bob").await;
    r.state().save(&slots, "bob").await.unwrap();
    let mut rules = f.rules.clone();
    rules.capability_grants.remove(&BOB_FORK_GRANT);
    f.moot
        .amend(f.alice.master_keypair().to_seed(), rules, 30)
        .await
        .unwrap();
    drop(r);
    drop(f);
    let f = disk(root.path()).await;
    let b = Mount::reopen(
        "reviewed",
        f.authority_at(&f.bob, 5_000, 2_000),
        Scope {
            collection: COLLECTION,
            fork_destination: Some(moot::resource_owner::ForkDestination {
                id: DESTINATION,
                name: "Freshly reviewed destination".into(),
            }),
        },
        &mut Owners {
            moot: &f.moot,
            identity: &f.bob,
            store: &mut f.blobs.clone(),
            revocations: &RevocationLedger::default(),
            at_ms: 2_000,
        },
        &slots,
        "bob",
    )
    .await
    .unwrap();
    assert_eq!(b.opened().unwrap().bytes, BODY);
    assert!(
        matches!(
            b.reading()
                .pane()
                .snapshot()
                .collection
                .as_ref()
                .unwrap()
                .fork,
            Some(Offer::Withheld { .. })
        ),
        "current grant revocation overrides old offered fork"
    );
    f.moot
        .update_membership_for_identity(
            &f.alice,
            MootMembershipAction::Remove {
                member: f.bob.master_public_key().to_bytes(),
            },
        )
        .await
        .unwrap();
    drop(b);
    drop(f);
    let f = disk(root.path()).await;
    assert!(
        reopen(
            &f,
            &f.bob,
            f.authority_at(&f.bob, 5_000, 2_000),
            &RevocationLedger::default(),
            &slots,
            "bob"
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("not a current Moot member")
    );
    assert_eq!(f.blobs.get("blob:notebook").await.unwrap().unwrap(), BODY);
}

#[tokio::test]
async fn withdrawn_selected_contribution_is_not_resurrected_from_retained_bytes() {
    let root = tempfile::tempdir().unwrap();
    let f = create(root.path()).await;
    let slots = RedbBackend::open(root.path().join("readings.redb")).unwrap();
    let r = opened(&f, &f.alice, "alice").await;
    let reference = r.state().opened().unwrap().reference;
    r.state().save(&slots, "alice").await.unwrap();
    f.moot
        .withdraw_share_for_identity(&f.alice, reference.contribution, 30)
        .await
        .unwrap();
    drop(r);
    drop(f);
    let f = disk(root.path()).await;
    let a = reopen(
        &f,
        &f.alice,
        f.authority_at(&f.alice, 5_000, 2_000),
        &RevocationLedger::default(),
        &slots,
        "alice",
    )
    .await
    .unwrap();
    assert!(a.opened().is_none() && a.reading().pane().selected_resource().is_none());
    assert!(a.reading().pane().snapshot().resources.is_empty());
    assert_eq!(f.blobs.get("blob:notebook").await.unwrap().unwrap(), BODY);
}

#[tokio::test]
async fn corrupted_or_missing_content_is_reverified_after_reopening() {
    let root = tempfile::tempdir().unwrap();
    let f = create(root.path()).await;
    let slots = RedbBackend::open(root.path().join("readings.redb")).unwrap();
    let r = opened(&f, &f.alice, "alice").await;
    r.state().save(&slots, "alice").await.unwrap();
    drop(r);
    drop(f);
    let f = disk(root.path()).await;
    for damaged in [Some(b"changed bytes".as_slice()), None] {
        if let Some(bytes) = damaged {
            f.blobs.put("blob:notebook", bytes).await.unwrap();
        } else {
            f.blobs.delete("blob:notebook").await.unwrap();
        }
        let a = reopen(
            &f,
            &f.alice,
            f.authority_at(&f.alice, 5_000, 2_000),
            &RevocationLedger::default(),
            &slots,
            "alice",
        )
        .await
        .unwrap();
        assert!(a.opened().is_none());
        assert_eq!(a.reading().pane().snapshot().resources.len(), 1);
    }
}

#[tokio::test]
async fn current_private_classification_refuses_reopen_and_clears_live_disclosure() {
    use eidetic::{
        ManifestId, PrivacyClass,
        manifest::{load_manifest, save_manifest},
    };
    let root = tempfile::tempdir().unwrap();
    let f = create(root.path()).await;
    let slots = RedbBackend::open(root.path().join("readings.redb")).unwrap();
    let mut r = opened(&f, &f.alice, "alice").await;
    r.state().save(&slots, "alice").await.unwrap();
    let mut store = f.blobs.clone();
    let mut manifest = load_manifest(&mut store, ManifestId::of_blob(BODY))
        .await
        .unwrap()
        .unwrap();
    manifest.privacy = PrivacyClass::MootScoped;
    save_manifest(&mut store, &manifest).await.unwrap();
    let a = reopen(
        &f,
        &f.alice,
        f.authority_at(&f.alice, 5_000, 2_000),
        &RevocationLedger::default(),
        &slots,
        "alice",
    )
    .await
    .unwrap();
    assert!(a.opened().is_none());
    r.update(|m| {
        m.request_refresh();
        pollster::block_on(f.poll(m, &f.alice, &RevocationLedger::default(), 23)).unwrap();
    });
    assert!(r.state().opened().is_none());
    assert_eq!(store.get("blob:notebook").await.unwrap().unwrap(), BODY);
}

#[tokio::test]
async fn fresh_admission_revocation_wrong_signer_and_slot_tampering_refuse_reopen() {
    use insigne::delegation::{DelegationRevocation, SignedDelegationRevocation};
    use personae::delegation::Issue;
    let root = tempfile::tempdir().unwrap();
    let f = create(root.path()).await;
    let slots = RedbBackend::open(root.path().join("readings.redb")).unwrap();
    let r = opened(&f, &f.alice, "alice").await;
    r.state().save(&slots, "alice").await.unwrap();
    let grant = f.grant(&f.alice, 5_000);
    let statement = SignedDelegationRevocation::issue(
        &f.alice,
        DelegationRevocation::new(
            grant.certificate.id(),
            f.alice.master_public_key().to_bytes(),
            grant.certificate.scope.clone(),
            30,
            [2; 32],
        ),
    )
    .unwrap();
    let mut ledger = RevocationLedger::new();
    ledger.fold(statement.check().unwrap());
    assert!(
        reopen(
            &f,
            &f.alice,
            f.authority_at(&f.alice, 5_000, 2_000),
            &ledger,
            &slots,
            "alice"
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("Revoked")
    );
    assert!(
        reopen(
            &f,
            &f.bob,
            f.authority_at(&f.alice, 5_000, 2_000),
            &RevocationLedger::default(),
            &slots,
            "alice"
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("signer")
    );
    assert!(
        reopen(
            &f,
            &f.bob,
            f.authority_at(&f.bob, 5_000, 2_000),
            &RevocationLedger::default(),
            &slots,
            "alice"
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("does not match")
    );
    let original = slots.get("alice").await.unwrap().unwrap();
    for (field, value) in [
        ("version", serde_json::json!(999)),
        ("moot", serde_json::json!(vec![0; 32])),
        ("collection", serde_json::json!(vec![0; 32])),
        ("authority", serde_json::json!({"grant":"invented"})),
    ] {
        let mut record: serde_json::Value = serde_json::from_slice(&original).unwrap();
        record[field] = value;
        slots
            .put("changed", &serde_json::to_vec(&record).unwrap())
            .await
            .unwrap();
        assert!(
            reopen(
                &f,
                &f.alice,
                f.authority_at(&f.alice, 5_000, 2_000),
                &RevocationLedger::default(),
                &slots,
                "changed"
            )
            .await
            .is_err(),
            "{field}"
        );
    }
    slots.put("changed", b"not JSON").await.unwrap();
    for key in ["changed", "absent"] {
        assert!(
            reopen(
                &f,
                &f.alice,
                f.authority_at(&f.alice, 5_000, 2_000),
                &RevocationLedger::default(),
                &slots,
                key
            )
            .await
            .is_err()
        );
    }
    assert_eq!(slots.get("alice").await.unwrap().unwrap(), original);
}
