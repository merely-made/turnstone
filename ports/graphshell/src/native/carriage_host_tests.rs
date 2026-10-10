// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Proof tests for the carriage host, split out for the file ceiling.
//!
//! A `#[path]` child of `carriage_host`, so private internals (`scan_held`,
//! the store field) stay reachable without widening their visibility.

use super::*;

mod lane {
    use super::*;
    use personae::InMemoryProvider;

    const GRAPH: [u8; 32] = [0x81; 32];

    fn issuer() -> Ed25519Keypair {
        Ed25519Keypair::from_seed([0x82; 32])
    }

    fn slot() -> BlindedSlotId {
        pandect::blinded_slot_id(insigne::delegation::DelegationId([0x83; 32]), [0x84; 32])
    }

    fn config(path: PathBuf, tickets: Vec<String>) -> CarriageHostConfig {
        CarriageHostConfig {
            graph: GRAPH,
            store_path: path,
            trusted_roots: vec![issuer().public_key().to_bytes()],
            ceilings: CarriageCeilings::default(),
            peer_tickets: tickets,
            paired_nodes: Vec::new(),
        }
    }

    /// The lane's whole point, demonstrated end to end: a peer that never
    /// held the record recovers it over the wire while the lease is live,
    /// without re-pairing, and a superseded version is replaced rather than
    /// accumulated.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_peer_recovers_a_live_slot_and_supersession_replaces_it() {
        let directory = tempfile::tempdir().unwrap();
        let wallet_device = InMemoryProvider::from_seed([0x85; 32]);
        let replica_device = InMemoryProvider::from_seed([0x86; 32]);

        let wallet = CarriageHost::open(
            &wallet_device,
            config(directory.path().join("wallet.redb"), Vec::new()),
        )
        .await
        .unwrap();
        let replica = CarriageHost::open(
            &replica_device,
            config(
                directory.path().join("replica.redb"),
                vec![wallet.ticket().await.unwrap()],
            ),
        )
        .await
        .unwrap();

        let lease_expiry = now_ms() + 60_000;
        wallet
            .publish_slot(
                &issuer(),
                slot(),
                1,
                lease_expiry,
                b"wrapped-record-v1".to_vec(),
                CarriageCeilings::default(),
            )
            .await
            .unwrap();

        // The replica learns the slot from sync alone; nothing hands it over.
        let mut recovered = None;
        for _ in 0..100 {
            recovered = replica.recover(slot()).await.unwrap();
            if recovered.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(
            recovered.as_deref(),
            Some(b"wrapped-record-v1".as_slice()),
            "the replica must serve back exactly what the wallet published"
        );

        // Supersession: issue 2 replaces issue 1 on the replica, and the
        // replica never accumulates history it could be harvested for.
        wallet
            .publish_slot(
                &issuer(),
                slot(),
                2,
                lease_expiry,
                b"wrapped-record-v2".to_vec(),
                CarriageCeilings::default(),
            )
            .await
            .unwrap();
        let mut superseded = None;
        for _ in 0..100 {
            superseded = replica.recover(slot()).await.unwrap();
            if superseded.as_deref() == Some(b"wrapped-record-v2".as_slice()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(superseded.as_deref(), Some(b"wrapped-record-v2".as_slice()));
        let stale = scan_held(&replica.store, GRAPH).await.unwrap();
        assert_eq!(
            stale.get(&slot()).map(|lease| lease.issue),
            Some(2),
            "the store holds the head version only"
        );

        wallet.close().await.unwrap();
        replica.close().await.unwrap();
    }

    /// Ruling 4's two enforcement points, on one host: an expired lease is
    /// refused on read, and the purge pass removes it from the store.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_expired_lease_is_refused_on_read_and_purged_on_schedule() {
        let directory = tempfile::tempdir().unwrap();
        let device = InMemoryProvider::from_seed([0x87; 32]);
        let host = CarriageHost::open(
            &device,
            config(directory.path().join("solo.redb"), Vec::new()),
        )
        .await
        .unwrap();

        host.publish_slot(
            &issuer(),
            slot(),
            1,
            now_ms() + 150,
            b"short-lease".to_vec(),
            CarriageCeilings::default(),
        )
        .await
        .unwrap();
        assert!(host.recover(slot()).await.unwrap().is_some());

        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        assert!(
            host.recover(slot()).await.unwrap().is_none(),
            "an expired lease must be refused on read"
        );

        let proposal = host.propose_purge().await;
        assert!(proposal.is_executable());
        assert_eq!(proposal.expired, vec![slot()]);
        let purged = host.execute_purge(&proposal).await.unwrap();
        assert!(purged >= 1, "the purge must delete the expired operation");
        assert_eq!(host.held_count().await, 0);

        host.close().await.unwrap();
    }

    /// Issue-side loudness: a lease violating a knowable ceiling is refused
    /// at the issuer, not silently dropped by every peer.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_ceiling_violation_is_refused_at_the_issuer() {
        let directory = tempfile::tempdir().unwrap();
        let device = InMemoryProvider::from_seed([0x88; 32]);
        let host = CarriageHost::open(
            &device,
            config(directory.path().join("ceiling.redb"), Vec::new()),
        )
        .await
        .unwrap();

        let refused = host
            .publish_slot(
                &issuer(),
                slot(),
                1,
                now_ms() + 60_000,
                b"too-long".to_vec(),
                CarriageCeilings {
                    device_max_ttl_ms: Some(1_000),
                    grant_expires_at_ms: None,
                },
            )
            .await;
        assert!(
            matches!(refused, Err(CarriageHostError::Refused(_))),
            "a lease over the device TTL must be refused at issue: {refused:?}"
        );
        host.close().await.unwrap();
    }
}

// The commissioning and retraction stories, which run through the real
// wallet, moved to djinn's `tests/carriage_commissioning.rs` in DR-C: the
// wallet is castellan's, and only djinn links castellan (D4, D15).

/// The endpoint fold: carriage attached to the personal sync host's bound
/// endpoint, both lanes converging over one connection per device.
mod attached {
    use super::*;
    use crate::native::personal_sync_host::{PersonalSyncHost, PersonalSyncHostConfig};
    use crate::personal_sync::{PersonalGraphEvent, SyncRoster, SyncSelection};
    use personae::{IdentityProvider, InMemoryProvider};
    use uuid::Uuid;

    const GRAPH: [u8; 32] = [0xB1; 32];

    fn issuer() -> Ed25519Keypair {
        Ed25519Keypair::from_seed([0xB2; 32])
    }

    fn slot() -> BlindedSlotId {
        pandect::blinded_slot_id(insigne::delegation::DelegationId([0xB3; 32]), [0xB4; 32])
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn both_lanes_converge_over_one_endpoint_per_device() {
        let directory = tempfile::tempdir().unwrap();
        let owner = InMemoryProvider::from_seed([0xB5; 32]);
        let sibling = InMemoryProvider::from_seed([0xB6; 32]);
        let roster = SyncRoster::new([
            owner.master_public_key().to_bytes(),
            sibling.master_public_key().to_bytes(),
        ]);
        let sync_config = |path: std::path::PathBuf, tickets: Vec<String>| PersonalSyncHostConfig {
            graph: GRAPH,
            store_path: path,
            roster: roster.clone(),
            selection: SyncSelection::default(),
            peer_tickets: tickets,
            peer_hints: Vec::new(),
            paired_nodes: Vec::new(),
            relay_urls: Vec::new(),
        };
        let owner_sync = PersonalSyncHost::open(
            &owner,
            sync_config(directory.path().join("owner.redb"), Vec::new()),
        )
        .await
        .unwrap();
        let sibling_sync = PersonalSyncHost::open(
            &sibling,
            sync_config(
                directory.path().join("sibling.redb"),
                vec![owner_sync.ticket().await.unwrap()],
            ),
        )
        .await
        .unwrap();
        owner_sync.pair_node(sibling_sync.node_id()).await.unwrap();

        // Attach carriage to BOTH existing endpoints: no second bind, and the
        // append-form overlay tag must leave the graph lane's tag standing.
        let trusted = vec![issuer().public_key().to_bytes()];
        let owner_carriage = CarriageHost::attach(
            &owner,
            &owner_sync,
            directory.path().join("owner-carriage.redb"),
            trusted.clone(),
            CarriageCeilings::default(),
            &[sibling_sync.node_id()],
        )
        .await
        .unwrap();
        let sibling_carriage = CarriageHost::attach(
            &sibling,
            &sibling_sync,
            directory.path().join("sibling-carriage.redb"),
            trusted,
            CarriageCeilings::default(),
            &[owner_sync.node_id()],
        )
        .await
        .unwrap();
        assert_eq!(
            owner_carriage.node_id(),
            owner_sync.node_id(),
            "an attached lane is the same endpoint, not a second identity"
        );

        // Carriage lane converges...
        owner_carriage
            .publish_slot(
                &issuer(),
                slot(),
                1,
                now_ms() + 60_000,
                b"folded-lane-record".to_vec(),
                CarriageCeilings::default(),
            )
            .await
            .unwrap();
        let mut recovered = None;
        for _ in 0..100 {
            recovered = sibling_carriage.recover(slot()).await.unwrap();
            if recovered.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(
            recovered.as_deref(),
            Some(b"folded-lane-record".as_slice()),
            "the carriage lane must converge over the shared endpoint"
        );

        // ...and the graph lane still does, which is what proves the overlay
        // tags composed instead of clobbering.
        owner_sync
            .author(vec![PersonalGraphEvent::AddNode {
                id: Uuid::from_u128(0xB7),
                address: "https://folded.test/".into(),
                title: "Folded-lane node".into(),
            }])
            .await
            .unwrap();
        let mut graph_converged = false;
        for _ in 0..100 {
            let cards = sibling_sync.supplemental_cards().await.unwrap();
            if cards
                .iter()
                .any(|card| card.card.title == "Folded-lane node")
            {
                graph_converged = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(
            graph_converged,
            "the graph lane must still converge beside carriage"
        );

        owner_carriage.close().await.unwrap();
        sibling_carriage.close().await.unwrap();
        owner_sync.close().await.unwrap();
        sibling_sync.close().await.unwrap();
    }
}
