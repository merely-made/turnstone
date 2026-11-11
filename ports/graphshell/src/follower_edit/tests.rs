// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::local_profile::{Browser, LocalProfile};
use muniment::{MemoryBackend, StoreError, TransactFn, WriteOp};

/// A store handle with every write refused, as `IndexedDbBackend::read_only`
/// is in the browser. Reads reach the shared bytes.
#[derive(Clone)]
struct ReadOnly(MemoryBackend);

fn refused() -> StoreError {
    StoreError::Backend("this store handle is read-only".into())
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl Backend for ReadOnly {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.0.get(key).await
    }
    async fn put(&self, _: &str, _: &[u8]) -> Result<(), StoreError> {
        Err(refused())
    }
    async fn delete(&self, _: &str) -> Result<(), StoreError> {
        Err(refused())
    }
    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        self.0.list(prefix).await
    }
    async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
        self.0.scan(start, end).await
    }
    async fn apply(&self, _: &[WriteOp]) -> Result<(), StoreError> {
        Err(refused())
    }
    async fn transact(&self, _: TransactFn) -> Result<(), StoreError> {
        Err(refused())
    }
}

fn profile() -> LocalProfile {
    LocalProfile::new(Uuid::from_u128(41), Browser::Chromium)
}

async fn bytes(store: &impl Backend) -> Vec<(String, Option<Vec<u8>>)> {
    let mut all = Vec::new();
    let mut keys = store.list("").await.unwrap();
    keys.sort();
    for key in keys {
        all.push((key.clone(), store.get(&key).await.unwrap()));
    }
    all
}

fn add(app: &GraphshellApp<ReadOnly>, id: &str, address: &str) -> Request {
    Request {
        id: id.into(),
        view: "view-a".into(),
        base: base_of(app),
        edit: Edit::AddNode {
            address: address.into(),
            title: "Added".into(),
        },
    }
}

fn retitle(app: &GraphshellApp<ReadOnly>, id: &str, member: Uuid, title: &str) -> Request {
    let held = metadata(app, member).unwrap();
    Request {
        id: id.into(),
        view: "view-a".into(),
        base: base_of(app),
        edit: Edit::EditNode {
            member,
            title: title.into(),
            tags: vec!["new".into()],
            was_title: held.title,
            was_tags: held.tags,
        },
    }
}

/// A writer holding one object, and a follower that has read it.
async fn pair() -> (
    MemoryBackend,
    GraphshellApp<MemoryBackend>,
    GraphshellApp<ReadOnly>,
    Uuid,
) {
    let store = MemoryBackend::new();
    let mut writer = GraphshellApp::open_local(store.clone(), &profile(), 1)
        .await
        .unwrap();
    let member = create_address(&mut writer, "https://example.org/", "Example").unwrap();
    persist_intake(&mut writer, member, 2).await.unwrap();
    let follower = GraphshellApp::observe_local(ReadOnly(store.clone()), &profile(), 3)
        .await
        .unwrap();
    (store, writer, follower, member)
}

#[test]
fn the_writer_admits_a_followers_new_object_and_the_follower_reads_it() {
    pollster::block_on(async {
        let (store, mut writer, follower, _) = pair().await;
        let request = add(&follower, "r1", "https://example.net/");
        let mut ledger = Admissions::new();
        let reply = ledger.admit(&mut writer, &request, 4).await;
        assert_eq!(reply.status, Status::Admitted, "{reply:?}");
        assert_eq!((reply.id.as_str(), reply.view.as_str()), ("r1", "view-a"));
        let member = reply.member.expect("the new object");
        assert!(writer.host.graph().get_node_by_id(member).is_some());
        // The store holds it: a fresh observation reads the object back.
        let reread = GraphshellApp::observe_local(ReadOnly(store), &profile(), 5)
            .await
            .unwrap();
        assert!(reread.host.graph().get_node_by_id(member).is_some());
        assert_eq!(reread.host.graph().node_count(), 2);
    });
}

#[test]
fn the_writer_admits_a_followers_title_and_tags_through_the_save_funnel() {
    pollster::block_on(async {
        let (_, mut writer, follower, member) = pair().await;
        let request = retitle(&follower, "r1", member, "Renamed");
        let reply = Admissions::new().admit(&mut writer, &request, 4).await;
        assert_eq!(reply.status, Status::Admitted, "{reply:?}");
        let held = metadata(&writer, member).unwrap();
        assert_eq!((held.title.as_str(), held.tags), ("Renamed", vec!["new".to_owned()]));
        assert!(!writer.host.has_unstored(), "stored before it is answered");
    });
}

#[test]
fn a_request_made_against_a_stale_base_is_refused_and_stores_nothing() {
    pollster::block_on(async {
        let (store, _writer, follower, member) = pair().await;
        // A new writer opens: the epoch moves on, and this follower has not reread.
        let mut next = GraphshellApp::open_local(store.clone(), &profile(), 6)
            .await
            .unwrap();
        let before = bytes(&store).await;
        let mut ledger = Admissions::new();
        for request in [
            add(&follower, "r1", "https://example.net/"),
            retitle(&follower, "r2", member, "Late"),
        ] {
            let reply = ledger.admit(&mut next, &request, 7).await;
            assert_eq!(reply.status, Status::Refused);
            assert!(reply.reason.starts_with("stale"), "{}", reply.reason);
        }
        assert_eq!(bytes(&store).await, before);
        // After rereading, the same edit is current.
        let reread = GraphshellApp::observe_local(ReadOnly(store), &profile(), 8)
            .await
            .unwrap();
        let reply = ledger
            .admit(&mut next, &retitle(&reread, "r3", member, "Late"), 9)
            .await;
        assert_eq!(reply.status, Status::Admitted, "{reply:?}");
    });
}

#[test]
fn an_object_that_moved_on_since_the_follower_read_it_refuses_the_edit() {
    pollster::block_on(async {
        let (store, mut writer, follower, member) = pair().await;
        crate::local_edit::save_metadata(&mut writer, member, "Changed", "", 4)
            .await
            .unwrap();
        let before = bytes(&store).await;
        let mut ledger = Admissions::new();
        let stale = retitle(&follower, "r1", member, "Mine");
        let reply = ledger.admit(&mut writer, &stale, 5).await;
        assert_eq!(reply.status, Status::Refused);
        assert!(reply.reason.contains("changed since"), "{}", reply.reason);
        assert_eq!(bytes(&store).await, before);
        // One that asks for what the object already is has nothing to refuse.
        let same = Request {
            edit: Edit::EditNode {
                member,
                title: "Changed".into(),
                tags: vec![],
                was_title: "Example".into(),
                was_tags: vec![],
            },
            id: "r2".into(),
            ..stale
        };
        let reply = ledger.admit(&mut writer, &same, 6).await;
        assert_eq!((reply.status, reply.reason.as_str()), (Status::Admitted, "already applied"));
        assert_eq!(bytes(&store).await, before, "an edit already true stores nothing");
        // A removed object is named, not guessed at.
        let gone = Request {
            id: "r3".into(),
            edit: Edit::EditNode {
                member: Uuid::from_u128(999),
                title: "x".into(),
                tags: vec![],
                was_title: String::new(),
                was_tags: vec![],
            },
            ..same
        };
        let reply = ledger.admit(&mut writer, &gone, 7).await;
        assert_eq!(reply.status, Status::Refused);
        assert!(reply.reason.contains("no longer exists"), "{}", reply.reason);
    });
}

#[test]
fn a_request_id_is_admitted_once_however_often_it_arrives() {
    pollster::block_on(async {
        let (store, mut writer, follower, _) = pair().await;
        let first = add(&follower, "same", "https://example.net/a");
        let mut ledger = Admissions::new();
        let one = ledger.admit(&mut writer, &first, 4).await;
        let stored = bytes(&store).await;
        let count = writer.host.graph().node_count();
        // The same request again, and the same id carrying something else.
        let two = ledger.admit(&mut writer, &first, 5).await;
        let other = Request {
            edit: Edit::AddNode {
                address: "https://example.net/b".into(),
                title: String::new(),
            },
            ..first.clone()
        };
        let three = ledger.admit(&mut writer, &other, 6).await;
        assert_eq!((&one, &one), (&two, &three));
        assert_eq!(writer.host.graph().node_count(), count);
        assert_eq!(bytes(&store).await, stored);
        // A writer that reloaded has no ledger; the address already held is
        // returned rather than added again.
        let reloaded = ledger.admit(&mut writer, &first, 7).await;
        assert_eq!(reloaded, one);
        let mut fresh = Admissions::new();
        let again = fresh.admit(&mut writer, &first, 8).await;
        assert_eq!(again.member, one.member);
        assert_eq!(writer.host.graph().node_count(), count);
    });
}

#[test]
fn the_ledger_forgets_its_oldest_answers_past_its_bound() {
    let mut ledger = Admissions::new();
    let request = Request::parse(
        r#"{"id":"x","view":"v","base":{"session":"00000000-0000-0000-0000-000000000001","epoch":1},
            "edit":{"kind":"add_node","address":"a","title":""}}"#,
    )
    .unwrap();
    for number in 0..REMEMBERED_REPLIES + 5 {
        let request = Request {
            id: number.to_string(),
            ..request.clone()
        };
        ledger.record(Reply::refused(&request, "r"));
    }
    assert!(ledger.answered("0").is_none());
    assert!(ledger.answered(&(REMEMBERED_REPLIES + 4).to_string()).is_some());
    assert_eq!(ledger.replies.len(), REMEMBERED_REPLIES);
}

#[test]
fn a_following_view_refuses_to_admit_and_a_read_only_store_refuses_every_write() {
    pollster::block_on(async {
        let (store, _writer, mut follower, _) = pair().await;
        let request = add(&follower, "r1", "https://example.net/");
        let mut ledger = Admissions::new();
        let Verdict::Answered(reply) = ledger.check(&follower, &request) else {
            panic!("a follower admits nothing");
        };
        assert_eq!(reply.status, Status::Refused);
        assert!(reply.reason.contains("does not hold the writer"));

        let before = bytes(&store).await;
        let read_only = ReadOnly(store.clone());
        assert!(read_only.put("k", b"v").await.is_err());
        assert!(read_only.delete("k").await.is_err());
        assert!(read_only.apply(&[]).await.is_err());
        // Reads are unchanged, and a host cannot take the writer through it.
        assert!(!read_only.list("").await.unwrap().is_empty());
        let error = GraphshellApp::open_local(read_only.clone(), &profile(), 9)
            .await
            .err()
            .expect("a read-only store cannot open as the writer");
        assert!(error.to_string().contains("read-only"), "{error}");
        // A follower's own attempt to store is refused and surfaced.
        assert!(create_address(&mut follower, "https://example.net/", "No").is_err());
        assert_eq!(bytes(&store).await, before, "the stored bytes did not move");
    });
}

#[test]
fn requests_and_replies_cross_the_wire_and_refuse_what_they_do_not_know() {
    let json = r#"{"id":"r","view":"v","base":{"session":"00000000-0000-0000-0000-000000000002","epoch":3},
        "edit":{"kind":"edit_node","member":"00000000-0000-0000-0000-000000000009",
        "title":"t","tags":["a"],"was_title":"","was_tags":[]}}"#;
    let request = Request::parse(json).unwrap();
    assert_eq!(request.base.epoch, 3);
    let reply = Reply::refused(&request, "no");
    assert_eq!(Reply::parse(&reply.to_json()).unwrap(), reply);
    assert!(Request::parse(&json.replace("\"epoch\":3", "\"epoch\":3,\"x\":1")).is_err());
    assert!(Request::parse(&json.replace("edit_node", "delete_everything")).is_err());
}
