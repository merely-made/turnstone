// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The coop owner attached to the ordinary `MereHost`. It stayed here when
//! `coop` moved to graphshell-host (engine split D88): `MereHost` and the
//! handler registry are the app's, and graphshell-host cannot name them.

use chirograph::Revision;
use graphshell_host::coop::{
    Contact, GraphAuthority, GraphReading, Operation, Owner, TERMS, hex, now_ms,
};
use mere::kernel::graph::{Author, apply::GraphDelta};
use personae::{IdentityProvider, InMemoryProvider};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[test]
fn attached_ordinary_host_is_the_only_writer_and_reopens_with_guest_authorship() {
    use crate::{
        handlers::HandlerRegistry,
        mere_host::{MereHost, SelectedPersonaRef},
    };
    use pandect::access::AccessContext;
    struct Attached(Arc<Mutex<MereHost<muniment::MemoryBackend>>>);
    impl GraphAuthority for Attached {
        fn read(&self) -> GraphReading {
            let host = self.0.lock().unwrap();
            let session = host.graph_session();
            GraphReading {
                graph: session.graph().clone(),
                revision: Revision(session.journal().live_cursor().0),
                authors: session.changes().iter().map(|c| c.author.clone()).collect(),
            }
        }
        fn transact(
            &self,
            expected: Revision,
            author: Author,
            deltas: Vec<GraphDelta>,
        ) -> Result<(), String> {
            pollster::block_on(
                self.0
                    .lock()
                    .unwrap()
                    .transact_coop(expected, author, deltas),
            )
            .map_err(|e| e.to_string())
        }
    }
    let backend = muniment::MemoryBackend::new();
    let selected = SelectedPersonaRef {
        persona: "ordinary-owner".into(),
        profile: "test".into(),
    };
    let context = AccessContext {
        persona: "ordinary-owner".into(),
        device: "test".into(),
        at_ms: 0,
    };
    let mut host = MereHost::empty(
        backend.clone(),
        selected.clone(),
        HandlerRegistry::new(vec![]),
        context.clone(),
    );
    pollster::block_on(host.persist(1)).unwrap();
    let first = Uuid::new_v4();
    pollster::block_on(host.transact_edits(
        "graphshell",
        vec![pandect::CapturedDelta::ReplayAddNodeWithIdIfMissing {
            id: first.to_string(),
            url: "https://existing.test/".into(),
            position: [0., 0.],
        }],
    ))
    .unwrap();
    let id = host.graph_session().id();
    let host = Arc::new(Mutex::new(host));
    let a = InMemoryProvider::from_seed([61; 32]);
    let b = InMemoryProvider::from_seed([62; 32]);
    let mut owner = pollster::block_on(Owner::attach(
        backend.clone(),
        id,
        a.master_public_key().to_bytes(),
        Arc::new(Attached(host.clone())),
    ))
    .unwrap();
    let invite =
        pollster::block_on(owner.invite(&a, Contact::create(&b, &TERMS).unwrap(), true, now_ms() + 60000))
            .unwrap();
    assert_eq!(owner.owner_data().nodes[0].id, first);
    let ordinary_revision = host.lock().unwrap().projection_revision();
    let edit = owner.owner_data().edit(Operation::Rename {
        id: first,
        title: "Guest revised existing source".into(),
    });
    pollster::block_on(owner.edit(b.master_public_key().to_bytes(), edit.clone())).unwrap();
    assert_eq!(
        host.lock().unwrap().graph().nodes().next().unwrap().1.title,
        "Guest revised existing source"
    );
    assert!(host.lock().unwrap().projection_revision() > ordinary_revision);
    assert!(pollster::block_on(owner.edit(b.master_public_key().to_bytes(), edit)).is_err());
    let second = Uuid::new_v4();
    pollster::block_on(host.lock().unwrap().transact_edits(
        "graphshell",
        vec![
            pandect::CapturedDelta::ReplayAddNodeWithIdIfMissing {
                id: second.to_string(),
                url: "https://ordinary-live.test/".into(),
                position: [1., 1.],
            },
            pandect::CapturedDelta::ReplayAssertRelationByIds {
                from_id: first.to_string(),
                to_id: second.to_string(),
                assertion: mere::kernel::graph::EdgeAssertion::Semantic {
                    sub_kind: mere::kernel::graph::SemanticSubKind::Cites,
                    label: None,
                },
            },
        ],
    ))
    .unwrap();
    assert_eq!(
        owner.owner_data().nodes.len(),
        2,
        "ordinary edits are visible without a second writer or reopen"
    );
    assert!(
        owner
            .owner_data()
            .relations
            .iter()
            .any(|r| r.from == first && r.to == second)
    );
    let guest_author = format!("personae:{}", hex(&b.master_public_key().to_bytes()));
    assert!(
        host.lock()
            .unwrap()
            .graph_session()
            .changes()
            .iter()
            .any(|c| c.author.id == guest_author
                && c.author.via.as_deref() == Some("graphshell.coop"))
    );
    pollster::block_on(owner.revoke(&a)).unwrap();
    let revision = owner.revision();
    drop(owner);
    drop(host);
    let host = pollster::block_on(MereHost::open_session(
        backend.clone(),
        id,
        selected,
        HandlerRegistry::new(vec![]),
        context,
    ))
    .unwrap();
    let host = Arc::new(Mutex::new(host));
    let owner = pollster::block_on(Owner::attach(
        backend,
        id,
        a.master_public_key().to_bytes(),
        Arc::new(Attached(host.clone())),
    ))
    .unwrap();
    assert_eq!(owner.revision(), revision);
    assert_eq!(owner.owner_data().nodes[0].id, first);
    assert!(
        owner
            .owner_data()
            .authors
            .iter()
            .any(|author| author.id == guest_author)
    );
    assert!(owner.guest_data(invite.guest.peer()).is_err());
}
