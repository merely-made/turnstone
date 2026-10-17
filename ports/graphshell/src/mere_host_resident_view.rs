// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The host's partial presentation (resident host plan, Task 4). The complete
//! writer stays the truth; settings and pane roots only choose what a reader
//! keeps resident, and `graph()` stays Full.
use super::*;
use crate::resident_host::{PaneRoots, ResidentSettings};
use pandect::addressable_graph::{GraphAddress, ResidencyPolicy};

impl<B: pandect::graph_session::LegacyTransactionBackend + Clone> MereHost<B> {
    pub fn resident_settings(&self) -> &ResidentSettings {
        &self.resident_view.settings
    }

    /// What to present: the installed partial reader when a Neighborhood policy
    /// is active, otherwise the complete writer. Editing and validation never
    /// use this graph.
    pub fn presentation_graph(&self) -> &Graph {
        match &self.resident_view.reader {
            Some(reader) if self.resident_view.settings.policy != ResidencyPolicy::Full => {
                reader.graph()
            },
            _ => self.graph(),
        }
    }

    /// Read this graph's settings, keeping the compatibility defaults when none
    /// were stored.
    pub async fn load_resident_settings(&mut self) -> Result<(), MereHostError> {
        let key =
            ResidentSettings::slot_key(*self.graph_session.manifest().root_graph_id.as_uuid());
        let stored: Option<ResidentSettings> = self.slots.load(&key).await?;
        if let Some(settings) = &stored {
            settings.validate()?;
        }
        self.resident_view.settings = stored.unwrap_or_default();
        self.refresh_resident().await
    }

    /// Store new settings and the matching residency policy in one atomic write,
    /// then rebuild the reader. Only the admitted host of a session may.
    pub async fn set_resident_settings(
        &mut self,
        settings: ResidentSettings,
    ) -> Result<(), MereHostError> {
        if !self.resident_publication {
            return Err(MereHostError::OwnershipUnproven);
        }
        settings.validate()?;
        let store = self.graph_session.resident_store();
        // A reader needs a publication. The first one bootstraps from the
        // session's own checkpoint; every later one rides an accepted edit.
        if !store.is_published().await? {
            self.graph_session.publish_resident_checkpoint().await?;
        }
        let key =
            ResidentSettings::slot_key(*self.graph_session.manifest().root_graph_id.as_uuid());
        let ops = vec![
            muniment::WriteOp::Put {
                key,
                value: serde_json::to_vec(&settings).map_err(|error| {
                    MereHostError::InvalidSnapshot(format!("resident settings: {error}"))
                })?,
            },
            store.policy_write(settings.policy)?,
        ];
        self.store()
            .transact(Box::new(move |_| ops))
            .await
            .map_err(MereHostError::Store)?;
        self.resident_view.settings = settings;
        self.resident_view.reader = None;
        self.refresh_resident().await
    }

    /// Replace one pane's roots (its authored roster and transient focus). The
    /// roster is kept whole even for members that are not resident.
    pub async fn set_pane_roots(
        &mut self,
        pane: ViewKey,
        roots: PaneRoots,
    ) -> Result<(), MereHostError> {
        self.resident_view.roots.set_pane(pane, roots);
        self.refresh_resident().await
    }

    /// Release one pane's roots; roots another pane or a pin still needs stay.
    pub async fn close_pane_roots(&mut self, pane: &ViewKey) -> Result<(), MereHostError> {
        self.resident_view.roots.close_pane(pane);
        self.refresh_resident().await
    }

    /// The graph's explicit pins, as roots.
    pub async fn set_resident_pins(
        &mut self,
        pins: std::collections::BTreeSet<GraphAddress>,
    ) -> Result<(), MereHostError> {
        self.resident_view.roots.set_pins(pins);
        self.refresh_resident().await
    }

    /// Bring the reader in line with the settings and the union of roots. A Full
    /// policy keeps no reader: the complete writer is the presentation.
    pub async fn refresh_resident(&mut self) -> Result<(), MereHostError> {
        let view = &mut self.resident_view;
        if view.settings.policy == ResidencyPolicy::Full {
            view.reader = None;
            return Ok(());
        }
        let roots = view.roots.union();
        match view.reader.as_mut() {
            Some(reader) => reader.reconcile(roots).await?,
            None => {
                view.reader = Some(
                    self.sessions
                        .open_resident_graph(self.graph_session.id(), roots)
                        .await?,
                );
            },
        }
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "native"))]
mod tests {
    use super::*;
    use crate::resident_host::ResidentRefreshMode;
    use muniment::MemoryBackend;
    use std::collections::BTreeSet;
    use uuid::Uuid;

    fn persona() -> SelectedPersonaRef {
        SelectedPersonaRef {
            persona: "task4".into(),
            profile: "fixture".into(),
        }
    }
    fn context() -> AccessContext {
        AccessContext {
            persona: "task4".into(),
            device: "fixture".into(),
            at_ms: 0,
        }
    }
    fn host_on(backend: &MemoryBackend, admitted: bool) -> MereHost<MemoryBackend> {
        let host = MereHost::empty(
            backend.clone(),
            persona(),
            HandlerRegistry::new(vec![]),
            context(),
        );
        if admitted {
            host.with_resident_publication()
        } else {
            host
        }
    }
    fn node(n: u128) -> CapturedDelta {
        CapturedDelta::ReplayAddNodeWithIdIfMissing {
            id: Uuid::from_u128(n).to_string(),
            url: format!("https://task4.test/{n}"),
            position: [0.0, 0.0],
        }
    }
    fn pane(name: &str) -> ViewKey {
        ViewKey::new("graphshell", name).unwrap()
    }
    async fn six_nodes(host: &mut MereHost<MemoryBackend>) {
        host.persist(1).await.unwrap();
        host.transact_edits("task4", (1..=6).map(node).collect())
            .await
            .unwrap();
        assert_eq!(host.graph().node_count(), 6);
    }
    fn neighborhood(hops: u16) -> ResidentSettings {
        ResidentSettings {
            policy: ResidencyPolicy::Neighborhood { hops },
            projection: Some("authored.review".into()),
            refresh: ResidentRefreshMode::Manual,
            publication_batch_ms: 0,
        }
    }

    #[test]
    fn the_presentation_follows_pane_roots_and_full_restores_the_writer() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut host = host_on(&backend, true);
            six_nodes(&mut host).await;
            // The compatibility setting presents the complete writer.
            assert_eq!(host.presentation_graph().node_count(), 6);
            host.set_resident_settings(neighborhood(0)).await.unwrap();
            // Scoped, but no pane has rooted anything yet.
            assert_eq!(host.presentation_graph().node_count(), 0);
            assert_eq!(host.graph().node_count(), 6, "graph() stays Full");
            let (a, b) = (Uuid::from_u128(1), Uuid::from_u128(2));
            host.set_pane_roots(
                pane("first"),
                PaneRoots {
                    roster: [a].into(),
                    focus: None,
                },
            )
            .await
            .unwrap();
            assert_eq!(host.presentation_graph().node_count(), 1);
            host.set_pane_roots(
                pane("second"),
                PaneRoots {
                    roster: [a, b].into(),
                    focus: Some(GraphAddress::Surface(Uuid::from_u128(3))),
                },
            )
            .await
            .unwrap();
            assert_eq!(
                host.presentation_graph().node_count(),
                3,
                "union of both panes"
            );
            host.set_resident_pins(BTreeSet::from([GraphAddress::Surface(Uuid::from_u128(6))]))
                .await
                .unwrap();
            assert_eq!(host.presentation_graph().node_count(), 4);
            // Closing a pane releases only what no one else needs.
            host.close_pane_roots(&pane("second")).await.unwrap();
            assert_eq!(
                host.presentation_graph().node_count(),
                2,
                "first's node plus the pin"
            );
            host.close_pane_roots(&pane("first")).await.unwrap();
            assert_eq!(host.presentation_graph().node_count(), 1, "the pin alone");
            // Full clears the limit and presents the writer again.
            host.set_resident_settings(ResidentSettings::default())
                .await
                .unwrap();
            assert_eq!(host.presentation_graph().node_count(), 6);
        });
    }

    #[test]
    fn settings_and_policy_are_stored_together_and_survive_a_reopen() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut host = host_on(&backend, true);
            six_nodes(&mut host).await;
            let tuned = ResidentSettings {
                refresh: ResidentRefreshMode::HostDriven {
                    minimum_check_ms: 750,
                },
                publication_batch_ms: 200,
                ..neighborhood(1)
            };
            host.set_resident_settings(tuned.clone()).await.unwrap();
            let id = host.graph_session().id();
            let mut reopened = MereHost::open_session_checked(
                backend.clone(),
                id,
                persona(),
                HandlerRegistry::new(vec![]),
                context(),
                None,
            )
            .await
            .unwrap();
            assert_eq!(reopened.resident_settings(), &ResidentSettings::default());
            reopened.load_resident_settings().await.unwrap();
            assert_eq!(reopened.resident_settings(), &tuned);
            // The residency policy the reader opens with is the stored one.
            assert_eq!(
                host.resident_view.reader.as_ref().unwrap().policy(),
                ResidencyPolicy::Neighborhood { hops: 1 }
            );
            assert_eq!(
                reopened.resident_view.reader.as_ref().unwrap().policy(),
                ResidencyPolicy::Neighborhood { hops: 1 }
            );
        });
    }

    #[test]
    fn settings_need_the_admitted_host_and_a_sound_shape_and_write_nothing_otherwise() {
        pollster::block_on(async {
            let backend = MemoryBackend::new();
            let mut stranger = host_on(&backend, false);
            stranger.persist(1).await.unwrap();
            let held = backend.list("").await.unwrap().len();
            assert!(matches!(
                stranger.set_resident_settings(neighborhood(1)).await,
                Err(MereHostError::OwnershipUnproven)
            ));
            let mut host = host_on(&MemoryBackend::new(), true);
            host.persist(1).await.unwrap();
            let zero = ResidentSettings {
                refresh: ResidentRefreshMode::HostDriven {
                    minimum_check_ms: 0,
                },
                ..neighborhood(1)
            };
            let before = host.store().list("").await.unwrap().len();
            assert!(matches!(
                host.set_resident_settings(zero).await,
                Err(MereHostError::Settings(_))
            ));
            assert_eq!(host.store().list("").await.unwrap().len(), before);
            assert_eq!(backend.list("").await.unwrap().len(), held);
            assert_eq!(host.resident_settings(), &ResidentSettings::default());
        });
    }
}
