// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Attached application writes install only after storage acknowledgement.
use super::*;

impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> MereHost<B> {
    pub async fn transact_edits(
        &mut self,
        via: &str,
        edits: Vec<CapturedDelta>,
    ) -> Result<(), MereHostError> {
        let author = author_of(&self.selected_persona, via);
        let deltas = edits
            .iter()
            .map(|edit| {
                edit.replay_delta_as(&author)
                    .ok_or_else(|| SessionError::NotReplayable(format!("{edit:?}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if self.resident_publication {
            let (_, generation) = self
                .graph_session
                .transact_deltas_published(author, deltas, wall_clock_now())
                .await?;
            self.resident_generation = Some(generation);
        } else {
            self.graph_session
                .transact_deltas(author, deltas, wall_clock_now())
                .await?;
        }
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(())
    }

    /// Apply observed stable-ID commands from a partial display, with their
    /// publication, in one atomic write. The author is this host's persona over
    /// `via`; nothing in the commands names one. Refused unless this is the
    /// admitted shared host of its session.
    pub async fn apply_observed_commands(
        &mut self,
        via: &str,
        commands: Vec<pandect::live_graph::ObservedCommand>,
    ) -> Result<pandect::live_graph::LiveApplied, MereHostError> {
        if !self.resident_publication {
            return Err(MereHostError::OwnershipUnproven);
        }
        let author = author_of(&self.selected_persona, via);
        let (applied, generation) = self
            .graph_session
            .transact_live(author, commands, wall_clock_now())
            .await?;
        self.resident_generation = Some(generation);
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(applied)
    }

    pub async fn transact_history(
        &mut self,
        via: &str,
        redo: bool,
    ) -> Result<Option<Reverted>, MereHostError> {
        let author = author_of(&self.selected_persona, via);
        let reverted = if self.resident_publication {
            let (reverted, generation) = self
                .graph_session
                .transact_history_published(author, redo, wall_clock_now())
                .await?;
            self.resident_generation = Some(generation);
            reverted
        } else {
            self.graph_session
                .transact_history(author, redo, wall_clock_now())
                .await?
        };
        self.bump_if(reverted.is_some());
        Ok(reverted)
    }

    pub async fn transact_view(
        &mut self,
        via: &str,
        view: &str,
        state: ViewIntent,
    ) -> Result<(), MereHostError> {
        let author = author_of(&self.selected_persona, via);
        let key = ViewKey::new(via, view)?;
        self.graph_session
            .transact_view(author, key, state, wall_clock_now())
            .await?;
        self.projection_revision = self.projection_revision.wrapping_add(1);
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "native"))]
mod tests {
    use super::*;
    use muniment::{MemoryBackend, StoreError, TransactFn, WriteOp};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(Clone)]
    struct RefusingBackend {
        inner: MemoryBackend,
        refuse: Arc<AtomicBool>,
    }
    #[async_trait::async_trait]
    impl Backend for RefusingBackend {
        async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
            self.inner.get(key).await
        }
        async fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
            self.inner.put(key, bytes).await
        }
        async fn delete(&self, key: &str) -> Result<(), StoreError> {
            self.inner.delete(key).await
        }
        async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
            self.inner.list(prefix).await
        }
        async fn scan(&self, start: &str, end: &str) -> Result<Vec<String>, StoreError> {
            self.inner.scan(start, end).await
        }
        async fn apply(&self, ops: &[WriteOp]) -> Result<(), StoreError> {
            self.inner.apply(ops).await
        }
        async fn transact(&self, f: TransactFn) -> Result<(), StoreError> {
            if self.refuse.load(Ordering::SeqCst) {
                Err(StoreError::Backend(
                    "injected atomic transaction refusal".into(),
                ))
            } else {
                self.inner.transact(f).await
            }
        }
    }
    async fn bytes(store: &MemoryBackend) -> BTreeMap<String, Vec<u8>> {
        let mut result = BTreeMap::new();
        for key in store.list("").await.unwrap() {
            result.insert(key.clone(), store.get(&key).await.unwrap().unwrap());
        }
        result
    }
    #[test]
    fn resident_transaction_keeps_graph_history_revision_and_bytes_on_storage_refusal_then_retries()
    {
        pollster::block_on(async {
            let store = RefusingBackend {
                inner: MemoryBackend::new(),
                refuse: Arc::new(AtomicBool::new(false)),
            };
            let selected = SelectedPersonaRef {
                persona: "c1-test".into(),
                profile: "fixture".into(),
            };
            let context = AccessContext {
                persona: "c1-test".into(),
                device: "fixture".into(),
                at_ms: 0,
            };
            let mut host = MereHost::empty(
                store.clone(),
                selected,
                HandlerRegistry::new(vec![]),
                context,
            )
            .with_session_item();
            host.persist(1).await.unwrap();
            let graph = serde_json::to_value(host.graph().to_snapshot_at(0)).unwrap();
            let facets = serde_json::to_value(host.graph().facets()).unwrap();
            let changes = host.graph_session().changes().to_vec();
            let cursor = host.graph_session().journal().live_cursor();
            let revision = host.projection_revision();
            let before = bytes(&store.inner).await;
            let edit = CapturedDelta::ReplayAddNodeWithIdIfMissing {
                id: uuid::Uuid::from_u128(1).to_string(),
                url: "https://c1.test/".into(),
                position: [0.0, 0.0],
            };
            store.refuse.store(true, Ordering::SeqCst);
            assert!(
                host.transact_edits("cleromancy", vec![edit.clone()])
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("injected atomic")
            );
            assert_eq!(
                serde_json::to_value(host.graph().to_snapshot_at(0)).unwrap(),
                graph
            );
            assert_eq!(serde_json::to_value(host.graph().facets()).unwrap(), facets);
            assert_eq!(host.graph_session().changes(), changes);
            assert_eq!(host.graph_session().journal().live_cursor(), cursor);
            assert_eq!(host.projection_revision(), revision);
            assert_eq!(bytes(&store.inner).await, before);
            // A second viewer reading the same resident host still receives
            // the acknowledged graph and revision, without optimistic edits.
            let shown = host.snapshot(host.local_request()).unwrap();
            assert_eq!(shown.scene.revision, revision);
            assert_eq!(shown.scene.active_items_in_order().len(), 1);
            store.refuse.store(false, Ordering::SeqCst);
            host.transact_edits("cleromancy", vec![edit]).await.unwrap();
            assert_eq!(host.graph().node_count(), 1);
            assert!(host.projection_revision().0 > revision.0);
            assert_eq!(
                host.graph_session()
                    .changes()
                    .last()
                    .unwrap()
                    .author
                    .via
                    .as_deref(),
                Some("cleromancy")
            );
            let changed = bytes(&store.inner).await;
            store.refuse.store(true, Ordering::SeqCst);
            let revision = host.projection_revision();
            assert!(host.transact_history("cleromancy", false).await.is_err());
            assert_eq!(host.graph().node_count(), 1);
            assert_eq!(host.projection_revision(), revision);
            assert_eq!(bytes(&store.inner).await, changed);
            store.refuse.store(false, Ordering::SeqCst);
            assert!(
                host.transact_history("cleromancy", false)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert_eq!(host.graph().node_count(), 0);
            assert!(
                host.transact_history("cleromancy", true)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert_eq!(host.graph().node_count(), 1);
            let before = bytes(&store.inner).await;
            let view_revision = host.projection_revision();
            store.refuse.store(true, Ordering::SeqCst);
            assert!(
                host.transact_view("cleromancy", "reading", ViewIntent::default())
                    .await
                    .is_err()
            );
            assert!(host.view("cleromancy", "reading").is_none());
            assert_eq!(host.projection_revision(), view_revision);
            assert_eq!(bytes(&store.inner).await, before);
            store.refuse.store(false, Ordering::SeqCst);
            host.transact_view("cleromancy", "reading", ViewIntent::default())
                .await
                .unwrap();
            assert!(host.view("cleromancy", "reading").is_some());
            let before = bytes(&store.inner).await;
            let view_revision = host.projection_revision();
            store.refuse.store(true, Ordering::SeqCst);
            let changed_view = ViewIntent {
                strategy: Some("phyllotaxis.default".into()),
                ..Default::default()
            };
            assert!(
                host.transact_view("cleromancy", "reading", changed_view)
                    .await
                    .is_err()
            );
            assert_eq!(
                host.view("cleromancy", "reading"),
                Some(&ViewIntent::default())
            );
            assert_eq!(host.projection_revision(), view_revision);
            assert_eq!(bytes(&store.inner).await, before);
            store.refuse.store(false, Ordering::SeqCst);
            // A durable head that changed outside this retained host is also
            // refused; its candidate never replaces acknowledged source.
            let mut competing = MereSessions::new(store.clone())
                .open(host.graph_session().id())
                .await
                .unwrap();
            let author = Author::person("c1-test").via("graphshell");
            competing
                .transact_deltas(
                    author.clone(),
                    vec![
                        CapturedDelta::ReplayAddNodeWithIdIfMissing {
                            id: uuid::Uuid::from_u128(8).to_string(),
                            url: "https://competing.test/".into(),
                            position: [0.0, 0.0],
                        }
                        .replay_delta_as(&author)
                        .unwrap(),
                    ],
                    wall_clock_now(),
                )
                .await
                .unwrap();
            let durable = bytes(&store.inner).await;
            let revision = host.projection_revision();
            assert!(
                host.transact_edits(
                    "cleromancy",
                    vec![CapturedDelta::ReplayAddNodeWithIdIfMissing {
                        id: uuid::Uuid::from_u128(9).to_string(),
                        url: "https://refused.test/".into(),
                        position: [0.0, 0.0]
                    }]
                )
                .await
                .unwrap_err()
                .to_string()
                .contains("Session changed before commit")
            );
            assert_eq!(host.graph().node_count(), 1);
            assert_eq!(host.projection_revision(), revision);
            assert_eq!(bytes(&store.inner).await, durable);
        });
    }

    fn resident_host(store: &RefusingBackend, admitted: bool) -> MereHost<RefusingBackend> {
        let host = MereHost::empty(
            store.clone(),
            SelectedPersonaRef {
                persona: "c1-test".into(),
                profile: "fixture".into(),
            },
            HandlerRegistry::new(vec![]),
            AccessContext {
                persona: "c1-test".into(),
                device: "fixture".into(),
                at_ms: 0,
            },
        )
        .with_session_item();
        if admitted {
            host.with_resident_publication()
        } else {
            host
        }
    }
    fn added(n: u128) -> CapturedDelta {
        CapturedDelta::ReplayAddNodeWithIdIfMissing {
            id: uuid::Uuid::from_u128(n).to_string(),
            url: format!("https://resident.test/{n}"),
            position: [0.0, 0.0],
        }
    }
    #[test]
    fn an_admitted_host_publishes_each_accepted_edit_and_undo_in_the_same_write() {
        pollster::block_on(async {
            let store = RefusingBackend {
                inner: MemoryBackend::new(),
                refuse: Arc::new(AtomicBool::new(false)),
            };
            let mut host = resident_host(&store, true);
            host.persist(1).await.unwrap();
            let id = host.graph_session().id();
            let reader = || async {
                MereSessions::new(store.clone())
                    .open_resident_graph(id, Default::default())
                    .await
                    .unwrap()
            };
            host.transact_edits("cleromancy", vec![added(1)])
                .await
                .unwrap();
            let first = host.resident_generation().unwrap().to_string();
            let view = reader().await;
            assert_eq!(view.generation(), first);
            assert_eq!(view.graph().node_count(), 1);
            // A refused write publishes nothing and leaves the last generation.
            let before = bytes(&store.inner).await;
            store.refuse.store(true, Ordering::SeqCst);
            assert!(
                host.transact_edits("cleromancy", vec![added(2)])
                    .await
                    .is_err()
            );
            assert!(host.transact_history("cleromancy", false).await.is_err());
            assert_eq!(bytes(&store.inner).await, before);
            assert_eq!(host.resident_generation(), Some(first.as_str()));
            assert_eq!(host.graph().node_count(), 1);
            store.refuse.store(false, Ordering::SeqCst);
            host.transact_history("cleromancy", false).await.unwrap();
            let undone = host.resident_generation().unwrap().to_string();
            assert_ne!(undone, first);
            let view = reader().await;
            assert_eq!(view.generation(), undone);
            assert_eq!(view.graph().node_count(), 0);
        });
    }
    #[test]
    fn observed_commands_need_the_admitted_host_and_carry_its_persona() {
        use pandect::addressable_graph::GraphAddress;
        use pandect::live_graph::{LiveCheck, LiveCommand, ObservedCommand};
        pollster::block_on(async {
            let store = RefusingBackend {
                inner: MemoryBackend::new(),
                refuse: Arc::new(AtomicBool::new(false)),
            };
            let id = uuid::Uuid::from_u128(11);
            let create = || {
                vec![ObservedCommand {
                    command: LiveCommand::CreateAddress {
                        id,
                        address: "https://resident.test/live".into(),
                        title: "Live".into(),
                    },
                    checks: vec![LiveCheck::Absent(GraphAddress::Surface(id))],
                }]
            };
            // Not the admitted host: refused, nothing written.
            let mut other = resident_host(&store, false);
            other.persist(1).await.unwrap();
            let before = bytes(&store.inner).await;
            assert!(matches!(
                other.apply_observed_commands("task2", create()).await,
                Err(MereHostError::OwnershipUnproven)
            ));
            assert_eq!(bytes(&store.inner).await, before);
            // The admitted host applies under its own persona and `via`.
            let mut host = resident_host(&store, true);
            host.persist(1).await.unwrap();
            let revision = host.projection_revision();
            let applied = host
                .apply_observed_commands("task2", create())
                .await
                .unwrap();
            assert_eq!(applied.created, vec![id]);
            assert!(host.projection_revision().0 > revision.0);
            let author = &host.graph_session().changes().last().unwrap().author;
            assert_eq!(author.via.as_deref(), Some("task2"));
            assert_eq!(host.graph().node_count(), 1);
            let view = MereSessions::new(store.clone())
                .open_resident_graph(host.graph_session().id(), Default::default())
                .await
                .unwrap();
            assert_eq!(Some(view.generation()), host.resident_generation());
            assert_eq!(view.graph().node_count(), 1);
            // A refused observation leaves the host as it was.
            let stale = vec![ObservedCommand {
                command: LiveCommand::SetPinned {
                    surface: id,
                    pinned: true,
                },
                checks: vec![LiveCheck::RecordUnchanged {
                    address: GraphAddress::Surface(id),
                    digest: "stale".into(),
                }],
            }];
            assert!(matches!(
                host.apply_observed_commands("task2", stale).await,
                Err(MereHostError::Live(
                    pandect::live_graph::LiveEditError::Stale(_)
                ))
            ));
        });
    }
    #[test]
    fn a_host_that_was_not_admitted_never_writes_a_publication() {
        pollster::block_on(async {
            let store = RefusingBackend {
                inner: MemoryBackend::new(),
                refuse: Arc::new(AtomicBool::new(false)),
            };
            let mut host = resident_host(&store, false);
            host.persist(1).await.unwrap();
            host.transact_edits("cleromancy", vec![added(1)])
                .await
                .unwrap();
            assert!(!host.resident_publication());
            assert_eq!(host.resident_generation(), None);
            assert!(
                bytes(&store.inner)
                    .await
                    .keys()
                    .all(|key| !key.contains("/resident/v1/")),
                "no addressable record without an admitted owner"
            );
        });
    }
}
