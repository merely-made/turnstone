// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Actual admitted host mounting and event-to-owner dispatch. Semantic DOMs,
//! signed in-memory owners; no layout/paint, persistence or transport claim.

#[path = "../examples/support/moot_resources_fixture.rs"]
mod fixture;
use cambium::{DomHandle, GenetAppRunner, PointerClick};
use fixture::{BOB_FORK_GRANT, BODY, COLLECTION, DESTINATION, Fixture};
use genet_scripted_dom::{NodeId, ScriptedDom};
use graphshell::moot_resources::{Mount, Owners, Scope, Update, View, view};
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use moot::resource_coop::{Offer, Outcome};
use notochord::{RetainedAuthority, RevocationLedger};
use personae::{IdentityProvider, InMemoryProvider};
use std::{cell::RefCell, rc::Rc};

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
fn inspect(runner: &mut Runner, scope: &str) {
    let reference = runner.state().reading().pane().snapshot().resources[0].reference;
    let hex: String = reference
        .contribution
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    click(runner, &format!("moot-{scope}-inspect-{hex}"));
}
fn poll(
    f: &Fixture,
    runner: &mut Runner,
    identity: &InMemoryProvider,
    ledger: &RevocationLedger,
    at_ms: u64,
) -> Result<Option<Update>, graphshell::moot_resources::Error> {
    let mut result = None;
    runner
        .update(|mount| result = Some(pollster::block_on(f.poll(mount, identity, ledger, at_ms))));
    result.unwrap()
}

#[tokio::test]
async fn mounted_clicks_open_and_fork_through_actual_owners_with_independent_readings() {
    let f = Fixture::new().await;
    let mut left = runner(f.mount("left", &f.alice).await);
    let mut right = runner(f.mount("right", &f.bob).await);
    let ledger = RevocationLedger::default();
    let bytes_before = f.blobs.len();
    inspect(&mut left, "left");
    click(&mut left, "moot-left-open");
    assert!(matches!(
        poll(&f, &mut left, &f.alice, &ledger, 21).unwrap(),
        Some(Update::Dispatched {
            outcome: Outcome::Accepted { .. },
            feedback_applied: true,
            refreshed: true
        })
    ));
    assert_eq!(left.state().opened().unwrap().bytes, BODY);
    assert!(right.state().opened().is_none());
    assert!(right.state().reading().pane().selected_resource().is_none());
    click(&mut right, "moot-right-fork-collection");
    assert!(matches!(
        poll(&f, &mut right, &f.bob, &ledger, 22).unwrap(),
        Some(Update::Dispatched {
            outcome: Outcome::Accepted { .. },
            refreshed: true,
            ..
        })
    ));
    let fork = f
        .moot
        .authorized_collection(gemot::moot::CollectionId(DESTINATION), 23)
        .await
        .unwrap()
        .unwrap();
    assert!(fork.fork.is_some());
    assert!(matches!(
        right
            .state()
            .reading()
            .pane()
            .snapshot()
            .collection
            .as_ref()
            .unwrap()
            .fork,
        Some(Offer::Withheld { .. })
    ));
    assert_eq!(
        f.blobs.len(),
        bytes_before,
        "reference fork writes no content"
    );
    assert_eq!(left.state().opened().unwrap().bytes, BODY);
}

#[tokio::test]
async fn current_constitution_revocation_refuses_queued_fork_and_refreshes_the_offer() {
    let mut f = Fixture::new().await;
    let mut mounted = runner(f.mount("bob", &f.bob).await);
    click(&mut mounted, "moot-bob-fork-collection");
    f.rules.capability_grants.remove(&BOB_FORK_GRANT);
    f.moot
        .amend(f.alice.master_keypair().to_seed(), f.rules.clone(), 30)
        .await
        .unwrap();
    assert!(matches!(
        poll(&f, &mut mounted, &f.bob, &RevocationLedger::default(), 31).unwrap(),
        Some(Update::Dispatched {
            outcome: Outcome::Refused { .. },
            refreshed: true,
            ..
        })
    ));
    assert!(
        f.moot
            .authorized_collection(gemot::moot::CollectionId(DESTINATION), 32)
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        mounted
            .state()
            .reading()
            .pane()
            .snapshot()
            .collection
            .as_ref()
            .unwrap()
            .fork,
        Some(Offer::Withheld { .. })
    ));
}

#[tokio::test]
async fn expired_session_clears_cached_disclosure_and_never_dispatches_queued_open() {
    let f = Fixture::new().await;
    let mut mounted = runner(f.mount("alice", &f.alice).await);
    inspect(&mut mounted, "alice");
    click(&mut mounted, "moot-alice-open");
    let count = f.blobs.len();
    assert!(
        poll(
            &f,
            &mut mounted,
            &f.alice,
            &RevocationLedger::default(),
            1_001
        )
        .is_err()
    );
    assert!(mounted.state().closed_reason().unwrap().contains("Expired"));
    assert!(
        mounted
            .state()
            .reading()
            .pane()
            .snapshot()
            .resources
            .is_empty()
    );
    assert!(mounted.state().opened().is_none());
    assert_eq!(f.blobs.len(), count);
}

#[tokio::test]
async fn explicit_refresh_invalidates_queued_command_and_idle_pumps_do_not_write() {
    let f = Fixture::new().await;
    let mut mounted = runner(f.mount("bob", &f.bob).await);
    click(&mut mounted, "moot-bob-fork-collection");
    click(&mut mounted, "moot-bob-refresh");
    assert!(matches!(
        poll(&f, &mut mounted, &f.bob, &RevocationLedger::default(), 21).unwrap(),
        Some(Update::Refreshed)
    ));
    for _ in 0..5 {
        assert!(
            poll(&f, &mut mounted, &f.bob, &RevocationLedger::default(), 22)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        f.moot
            .authorized_collection(gemot::moot::CollectionId(DESTINATION), 23)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_different_signer_cannot_reuse_the_mounted_session() {
    let f = Fixture::new().await;
    let mut mounted = runner(f.mount("alice", &f.alice).await);
    click(&mut mounted, "moot-alice-fork-collection");
    assert!(poll(&f, &mut mounted, &f.bob, &RevocationLedger::default(), 21).is_err());
    assert!(mounted.state().closed_reason().unwrap().contains("signer"));
    assert!(
        f.moot
            .authorized_collection(gemot::moot::CollectionId(DESTINATION), 22)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn other_service_admission_is_not_a_moot_host_mount() {
    let f = Fixture::new().await;
    let admitted = f.authority(&f.alice, 1_000);
    let mut principal = admitted.principal().clone();
    principal.action.path = "/services/murm".into();
    let mut store = f.blobs.clone();
    let result = Mount::open(
        "wrong-service",
        RetainedAuthority::new(principal, vec![f.grant(&f.alice, 1_000)]),
        Scope {
            collection: COLLECTION,
            fork_destination: None,
        },
        &mut Owners {
            moot: &f.moot,
            identity: &f.alice,
            store: &mut store,
            revocations: &RevocationLedger::default(),
            at_ms: 20,
        },
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("another service"));
}

#[tokio::test]
async fn signed_session_revocation_clears_an_opened_projection_without_deleting_content() {
    use insigne::delegation::{DelegationRevocation, SignedDelegationRevocation};
    use personae::delegation::Issue;
    let f = Fixture::new().await;
    let mut mounted = runner(f.mount("alice", &f.alice).await);
    inspect(&mut mounted, "alice");
    click(&mut mounted, "moot-alice-open");
    poll(&f, &mut mounted, &f.alice, &RevocationLedger::default(), 21).unwrap();
    assert_eq!(mounted.state().opened().unwrap().bytes, BODY);
    let certificate = f.grant(&f.alice, 1_000);
    let statement = SignedDelegationRevocation::issue(
        &f.alice,
        DelegationRevocation::new(
            certificate.certificate.id(),
            f.alice.master_public_key().to_bytes(),
            certificate.certificate.scope.clone(),
            22,
            [2; 32],
        ),
    )
    .unwrap();
    let mut ledger = RevocationLedger::new();
    ledger.fold(statement.check().unwrap());
    let count = f.blobs.len();
    assert!(poll(&f, &mut mounted, &f.alice, &ledger, 23).is_err());
    assert!(mounted.state().closed_reason().unwrap().contains("Revoked"));
    assert!(mounted.state().opened().is_none());
    assert!(
        mounted
            .state()
            .reading()
            .pane()
            .snapshot()
            .resources
            .is_empty()
    );
    assert_eq!(f.blobs.len(), count);
}

#[tokio::test]
async fn current_moot_removal_refuses_a_queued_open_and_closes_the_owner_read() {
    let f = Fixture::new().await;
    let mut mounted = runner(f.mount("bob", &f.bob).await);
    inspect(&mut mounted, "bob");
    click(&mut mounted, "moot-bob-open");
    f.moot
        .update_membership_for_identity(
            &f.alice,
            gemot::moot::MootMembershipAction::Remove {
                member: f.bob.master_public_key().to_bytes(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        poll(&f, &mut mounted, &f.bob, &RevocationLedger::default(), 23).unwrap(),
        Some(Update::Dispatched {
            outcome: Outcome::Refused { .. },
            refreshed: false,
            ..
        })
    ));
    assert!(
        mounted
            .state()
            .closed_reason()
            .unwrap()
            .contains("current Moot member")
    );
    assert!(mounted.state().opened().is_none());
    assert!(
        mounted
            .state()
            .reading()
            .pane()
            .snapshot()
            .resources
            .is_empty()
    );
}
