// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's public evidence-brief embedding. The existing MereHost is its
//! sole graph writer. Example principals are synthetic and grant no private data.

use crate::{
    access::AccessContext,
    mere_host::{MereHost, SelectedPersonaRef, fixture_handlers},
};
use alembic::{
    workshop::*,
    workshop_surface::{RetainedReading, SourceChoice, SurfaceAction, SurfaceState},
};
use athanor::{workshop, workshop_driver::Driver};
use mere::kernel::graph::apply::GraphDelta;
use muniment::DirectoryBackend;
use servitor::{Grant, GrantTable, Mode, Subject};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

type Store = DirectoryBackend;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn failure(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn encoded_owner(owner: &Owner) -> Vec<u8> {
    serde_json::to_vec(owner).expect("owner serializes")
}
fn surface_id(owner: &Owner, index: usize) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        &encoded_owner(&Owner {
            host: format!("{}/public-example/{index}", owner.host),
            ..owner.clone()
        }),
    )
}
fn source_iri(owner: &Owner, index: usize) -> String {
    format!(
        "https://example.invalid/alembic/{}/{}",
        owner.session,
        surface_id(owner, index)
    )
}
fn source_resource(owner: &Owner, index: usize) -> String {
    chartulary::resource_id_from_canonical_iri(&chartulary::canonical_url(&source_iri(
        owner, index,
    )))
    .to_string()
}
fn public_passage(owner: &Owner, passage: &Passage) -> bool {
    passage.public
        && passage.source.owner == *owner
        && PUBLIC_EXAMPLE
            .iter()
            .enumerate()
            .any(|(i, (_, _, title, text))| {
                passage.source.resource == source_resource(owner, i)
                    && passage.source.title == *title
                    && passage.text == *text
                    && passage.source.start == 0
                    && passage.source.end == text.len() as u64
            })
}

pub struct GraphshellWorkshop {
    host: MereHost<Store>,
    driver: Driver<Store>,
    owner: Owner,
    installation: [u8; 16],
    helper: [u8; 32],
    human: [u8; 32],
    authority: GrantTable,
    // The owner lease is acquired before reading either journal. An abandoned
    // process releases it through the OS; its old worker cannot still be live.
    _writer_lease: File,
}

impl GraphshellWorkshop {
    pub async fn open(root: &Path, wake: Arc<dyn Fn() + Send + Sync>) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(failure)?;
        let writer_lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("workshop-owner.lock"))
            .map_err(failure)?;
        writer_lease
            .try_lock()
            .map_err(|error| format!("This workshop already has a writer: {error}"))?;
        let backend = DirectoryBackend::open(root.join("graph-owner")).map_err(failure)?;
        let mut host = MereHost::open(
            backend,
            SelectedPersonaRef {
                persona: "public-example".into(),
                profile: "alembic".into(),
            },
            fixture_handlers(),
            AccessContext {
                persona: "public-example".into(),
                device: "local-example".into(),
                at_ms: now_ms(),
            },
        )
        .await
        .map_err(failure)?;
        host.workshop_initialize_recorded().await.map_err(failure)?;
        let mut host = host.with_resident_publication();
        host.persist(now_ms() / 1000).await.map_err(failure)?;
        let owner = host.workshop_owner();
        let seed = encoded_owner(&owner);
        let helper = *blake3::hash(&[seed.as_slice(), b"/public-helper"].concat()).as_bytes();
        let human = *blake3::hash(&[seed.as_slice(), b"/synthetic-reviewer"].concat()).as_bytes();
        let installation = *Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            &[seed.as_slice(), b"/installation"].concat(),
        )
        .as_bytes();
        let store = DirectoryBackend::open(root.join("workshop")).map_err(failure)?;
        let driver = Driver::new(store, owner.clone(), installation, wake);
        driver.open(helper).await.map_err(failure)?;
        let mut this = Self {
            host,
            driver,
            owner,
            installation,
            helper,
            human,
            authority: GrantTable::new(),
            _writer_lease: writer_lease,
        };
        let view = this.driver.refresh().await.map_err(failure)?;
        if let Some(id) = view.run {
            let inputs = this.driver.workshop.run_inputs(id).await.map_err(failure)?;
            this.grant(&inputs)?;
        }
        this.driver
            .recover(&this.authority, &now_ms)
            .await
            .map_err(failure)?;
        Ok(this)
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn graph(&self) -> &mere::kernel::graph::Graph {
        self.host.graph()
    }
    pub fn busy(&self) -> bool {
        self.driver.busy()
    }

    fn destination(&self) -> Result<Destination, String> {
        Ok(Destination {
            owner: self.owner.clone(),
            graph: self.owner.workspace.clone(),
            revision: self.host.workshop_revision().map_err(failure)?,
        })
    }
    fn grant(&mut self, inputs: &[Passage]) -> Result<(), String> {
        let mut authority = GrantTable::new();
        for input in inputs {
            if !public_passage(&self.owner, input) {
                return Err("This helper supports the authored public example only; private sources remain unavailable.".into());
            }
            authority = authority.with_grant(Grant::new(
                Subject(self.helper),
                workshop::read_cap(&input.source),
                Mode::Read,
            ));
        }
        authority = authority.with_grant(Grant::new(
            Subject(self.helper),
            workshop::proposal_cap(&self.owner, self.installation),
            Mode::Write,
        ));
        authority = authority.with_grant(Grant::new(
            Subject(self.human),
            workshop::destination_cap(&self.destination()?),
            Mode::Write,
        ));
        self.authority = authority;
        Ok(())
    }
    fn capture(&self, key: &str) -> Result<Passage, String> {
        let id = Uuid::parse_str(key).map_err(failure)?;
        let index = (0..3)
            .find(|i| surface_id(&self.owner, *i) == id)
            .ok_or("Source is outside the public example")?;
        let (_, _, title, text) = PUBLIC_EXAMPLE[index];
        let snapshot = self.host.graph().to_snapshot_at(0);
        let node = snapshot
            .nodes
            .iter()
            .find(|n| n.node_id == id.to_string())
            .ok_or("Retained source is missing")?;
        if node.title != title
            || node.body.as_deref() != Some(text)
            || node.url != source_iri(&self.owner, index)
        {
            return Err(
                "The example source changed; its new bytes have no public admission.".into(),
            );
        }
        self.host
            .workshop_capture_public(id, 0, text.len() as u64, |surface, resource| {
                surface == id && resource.to_string() == source_resource(&self.owner, index)
            })
            .map_err(failure)
    }
    // The owner and graph cannot change while its synchronous service turn
    // awaits the local backend. Re-capture every version for each driver turn;
    // citation opening separately permits the exact admitted retained old copy.
    fn current_passages(&self) -> Vec<Passage> {
        (0..3)
            .filter_map(|i| self.capture(&surface_id(&self.owner, i).to_string()).ok())
            .collect()
    }
    fn choices(&self) -> Vec<SourceChoice> {
        (0..3)
            .filter_map(|i| {
                let id = surface_id(&self.owner, i).to_string();
                if self
                    .host
                    .graph()
                    .get_node_by_id(surface_id(&self.owner, i))
                    .is_none()
                {
                    return None;
                }
                let captured = self.capture(&id);
                Some(SourceChoice {
                    key: id,
                    title: format!("{} · {}", PUBLIC_EXAMPLE[i].0, PUBLIC_EXAMPLE[i].2),
                    version: captured
                        .as_ref()
                        .map(|p| p.source.version.clone())
                        .unwrap_or_else(|_| "unavailable".into()),
                    selected: true,
                    available: captured.is_ok(),
                    refusal: captured.err(),
                })
            })
            .collect()
    }
    async fn load_example(&mut self, state: &mut SurfaceState) -> Result<(), String> {
        let mut deltas = vec![];
        for (i, (_, _, title, text)) in PUBLIC_EXAMPLE.iter().enumerate() {
            let id = surface_id(&self.owner, i);
            if self.host.graph().get_node_by_id(id).is_some() {
                self.capture(&id.to_string())?;
                continue;
            }
            deltas.extend([
                GraphDelta::AddNode {
                    id: Some(id),
                    url: source_iri(&self.owner, i),
                    position: (i as f32 * 220., 0.).into(),
                },
                GraphDelta::ReplaySetNodeTitleById {
                    node_id: id,
                    title: (*title).into(),
                },
                GraphDelta::ReplaySetNodeBodyById {
                    node_id: id,
                    body: Some((*text).into()),
                },
                GraphDelta::ReplaySetNodeMimeHintById {
                    node_id: id,
                    mime_hint: Some("text/plain".into()),
                },
            ]);
        }
        if !deltas.is_empty() {
            self.host.workshop_seed(deltas).await.map_err(failure)?;
        }
        state.choices = self.choices();
        state.status = "Public example retained in this Graphshell graph. The reviewer identity is synthetic; private sources are excluded.".into();
        Ok(())
    }
    async fn handle(
        &mut self,
        action: SurfaceAction,
        state: &mut SurfaceState,
    ) -> Result<(), String> {
        match action {
            SurfaceAction::LoadExample => self.load_example(state).await?,
            SurfaceAction::Run => {
                let inputs = state
                    .selected()
                    .iter()
                    .map(|key| self.capture(key))
                    .collect::<Result<Vec<_>, _>>()?;
                self.grant(&inputs)?;
                let current = self.current_passages();
                self.driver
                    .begin(inputs, &self.authority, &now_ms, &|p| current.contains(p))
                    .await
                    .map_err(failure)?;
                state.reading = None;
                state.status = "Preparing retained evidence…".into();
            },
            SurfaceAction::Control(Control::Accept) => {
                let view = self.driver.refresh().await.map_err(failure)?;
                let id = view.run.ok_or("No proposal run")?;
                let proposal = view.proposal.ok_or("No saved proposal")?;
                let current = self.current_passages();
                if !proposal
                    .sources
                    .iter()
                    .all(|source| current.contains(source))
                {
                    return Err("A selected source version changed. Review a new proposal before adding it.".into());
                }
                let intent = self
                    .driver
                    .workshop
                    .accept(
                        id,
                        self.destination()?,
                        self.human,
                        &self.authority,
                        now_ms(),
                    )
                    .await
                    .map_err(failure)?;
                match self
                    .host
                    .workshop_commit(
                        &intent,
                        &proposal,
                        self.human,
                        &self.authority,
                        now_ms(),
                        true,
                    )
                    .await
                {
                    Ok(receipt) => {
                        self.driver
                            .workshop
                            .acknowledge(id, receipt)
                            .await
                            .map_err(failure)?;
                        state.status = "Brief added; Graphshell confirmed durable storage.".into();
                    },
                    Err(error) => {
                        self.driver.workshop.uncertain(id).await.map_err(failure)?;
                        return Err(format!("Destination outcome needs reconciliation: {error}"));
                    },
                }
            },
            SurfaceAction::Control(Control::Reject) => {
                let id = self
                    .driver
                    .refresh()
                    .await
                    .map_err(failure)?
                    .run
                    .ok_or("No proposal run")?;
                self.driver.workshop.reject(id).await.map_err(failure)?;
                state.status = "Brief rejected.".into();
            },
            SurfaceAction::Control(Control::Reconcile) => {
                let view = self.driver.refresh().await.map_err(failure)?;
                let id = view.run.ok_or("No proposal run")?;
                let proposal = view.proposal.ok_or("No saved proposal")?;
                let intent = match view.review {
                    Some(
                        Review::Applying(a)
                        | Review::Reconciling(a)
                        | Review::Added { acceptance: a, .. },
                    ) => a,
                    _ => return Err("No pending destination operation".into()),
                };
                match self
                    .host
                    .workshop_reconcile(&intent, &proposal)
                    .await
                    .map_err(failure)?
                {
                    Some(receipt) => {
                        self.driver
                            .workshop
                            .acknowledge(id, receipt)
                            .await
                            .map_err(failure)?;
                        state.status = "Recovered the original destination receipt.".into();
                    },
                    None => {
                        self.driver.workshop.uncertain(id).await.map_err(failure)?;
                        state.status="No durable receipt found. The outcome stays unresolved; no new edit was attempted.".into();
                    },
                }
            },
            SurfaceAction::Control(control) => {
                let current = self.current_passages();
                self.driver
                    .control(control, &self.authority, &now_ms, &|p| current.contains(p))
                    .await
                    .map_err(failure)?;
            },
            SurfaceAction::OpenSource { proposal, source } => {
                let view = self.driver.refresh().await.map_err(failure)?;
                let held = view.proposal.ok_or("No saved proposal")?;
                if held.digest != proposal {
                    return Err("The source request belongs to another proposal".into());
                }
                let passage = held
                    .sources
                    .iter()
                    .find(|p| p.source == source && public_passage(&self.owner, p))
                    .ok_or("That source version is unavailable")?;
                state.reading = Some(RetainedReading {
                    title: passage.source.title.clone(),
                    version: format!(
                        "{} · bytes {}–{}",
                        passage.source.version, passage.source.start, passage.source.end
                    ),
                    text: passage.text.clone(),
                });
            },
            SurfaceAction::OpenResult(resource) => {
                let view = self.driver.refresh().await.map_err(failure)?;
                let Some(Review::Added {
                    acceptance,
                    receipt,
                }) = view.review
                else {
                    return Err("No acknowledged note".into());
                };
                if receipt.resource != resource {
                    return Err("The result belongs to another operation".into());
                }
                let graph = self.host.graph();
                let id = athanor::workshop_destination::result_surface(&acceptance);
                let (key, _) = graph
                    .get_node_by_id(id)
                    .ok_or("The accepted note has been removed; its receipt remains retained")?;
                if graph
                    .shown_resource_id(key)
                    .is_none_or(|r| r.to_string() != resource)
                {
                    return Err(
                        "The original result surface changed; its receipt remains retained".into(),
                    );
                }
                let snapshot = graph.to_snapshot_at(0);
                let note = snapshot
                    .nodes
                    .iter()
                    .find(|n| n.node_id == id.to_string())
                    .ok_or("The retained note is missing")?;
                let text = note
                    .body
                    .clone()
                    .ok_or("The retained note body is missing")?;
                state.reading = Some(RetainedReading {
                    title: note.title.clone(),
                    version: format!(
                        "{} · original operation {}",
                        blake3::hash(text.as_bytes()).to_hex(),
                        receipt.storage_witness
                    ),
                    text,
                });
            },
        }
        Ok(())
    }

    pub async fn refresh_surface(&self, state: &mut SurfaceState) -> Result<(), String> {
        if state.choices.is_empty() {
            state.choices = self.choices();
        }
        state.view = Some(self.driver.refresh().await.map_err(failure)?);
        state.busy = self.driver.busy();
        Ok(())
    }
    /// Native hosts call this on normal UI dispatch and worker wake. The worker
    /// receive is nonblocking; source/current authority are supplied each time.
    pub fn service(&mut self, state: &mut SurfaceState) -> bool {
        pollster::block_on(async {
            let actions = state.take_actions();
            let mut changed = !actions.is_empty();
            for action in actions {
                state.error = None;
                if let Err(error) = self.handle(action, state).await {
                    state.error = Some(error);
                }
            }
            let current = self.current_passages();
            match self
                .driver
                .poll(&self.authority, &now_ms, &|p| current.contains(p))
                .await
            {
                Ok(Some(view)) => {
                    changed = true;
                    state.status = match view.phase {
                        RunStatus::Completed => "Proposal saved for review.",
                        RunStatus::Ready(_) | RunStatus::Awaiting(_) => {
                            "Preparing retained evidence…"
                        },
                        RunStatus::Paused(_) => "Helper paused.",
                        _ => "Helper state updated.",
                    }
                    .into();
                },
                Ok(None) => {},
                Err(error) => {
                    changed = true;
                    state.error = Some(error.to_string());
                },
            }
            if changed {
                if let Err(error) = self.refresh_surface(state).await {
                    state.error = Some(error);
                }
            }
            changed
        })
    }
    pub fn close(&mut self) -> Result<(), String> {
        pollster::block_on(self.driver.shutdown(&self.authority, &now_ms)).map_err(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn open(root: &Path) -> GraphshellWorkshop {
        pollster::block_on(GraphshellWorkshop::open(root, Arc::new(|| {}))).unwrap()
    }
    fn generated(owner: &mut GraphshellWorkshop) -> SurfaceState {
        let mut state = SurfaceState::new();
        state.request(SurfaceAction::LoadExample);
        assert!(owner.service(&mut state));
        assert!(state.error.is_none(), "{:?}", state.error);
        state.request(SurfaceAction::Run);
        owner.service(&mut state);
        let stop = Instant::now() + Duration::from_secs(5);
        while owner.busy() && Instant::now() < stop {
            owner.service(&mut state);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!owner.busy(), "worker did not finish");
        assert!(state.error.is_none(), "{:?}", state.error);
        assert_eq!(state.view.as_ref().unwrap().phase, RunStatus::Completed);
        state
    }

    #[test]
    fn workshop_graphshell_acceptance_and_exact_citations_survive_cold_owner_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let mut owner = open(directory.path());
        let mut state = generated(&mut owner);
        let proposal = state.view.as_ref().unwrap().proposal.clone().unwrap();
        assert_eq!(proposal.sources.len(), 3);
        for (passage, (_, _, title, text)) in proposal.sources.iter().zip(PUBLIC_EXAMPLE) {
            assert_eq!(passage.text, text);
            assert_eq!(passage.source.title, title);
        }
        let source = proposal.sources[1].source.clone();
        state.request(SurfaceAction::OpenSource {
            proposal: proposal.digest,
            source,
        });
        owner.service(&mut state);
        assert_eq!(state.reading.as_ref().unwrap().text, PUBLIC_EXAMPLE[1].3);
        state.request(SurfaceAction::Control(Control::Accept));
        owner.service(&mut state);
        assert!(state.error.is_none(), "{:?}", state.error);
        let review = state.view.as_ref().unwrap().review.clone().unwrap();
        assert!(matches!(&review, Review::Added { .. }));
        let id = owner.owner().clone();
        assert_eq!(owner.graph().nodes().count(), 4);
        owner.close().unwrap();
        drop(owner);
        let mut reopened = open(directory.path());
        let mut state = SurfaceState::new();
        pollster::block_on(reopened.refresh_surface(&mut state)).unwrap();
        assert_eq!(reopened.owner(), &id);
        assert_eq!(
            state.view.as_ref().unwrap().proposal.as_ref(),
            Some(&proposal)
        );
        assert_eq!(state.view.as_ref().unwrap().review.as_ref(), Some(&review));
        let Review::Added { receipt, .. } = review else {
            unreachable!()
        };
        state.request(SurfaceAction::OpenResult(receipt.resource));
        reopened.service(&mut state);
        assert!(state.error.is_none(), "{:?}", state.error);
        assert_eq!(state.reading.unwrap().text, proposal.djot);
        assert_eq!(reopened.graph().nodes().count(), 4);
    }

    #[test]
    fn workshop_graphshell_lost_ack_queries_original_operation_without_repeating_edit() {
        let directory = tempfile::tempdir().unwrap();
        let mut owner = open(directory.path());
        let state = generated(&mut owner);
        let view = state.view.unwrap();
        let id = view.run.unwrap();
        let proposal = view.proposal.unwrap();
        let destination = owner.destination().unwrap();
        let acceptance = pollster::block_on(owner.driver.workshop.accept(
            id,
            destination,
            owner.human,
            &owner.authority,
            now_ms(),
        ))
        .unwrap();
        let receipt = pollster::block_on(owner.host.workshop_commit(
            &acceptance,
            &proposal,
            owner.human,
            &owner.authority,
            now_ms(),
            true,
        ))
        .unwrap();
        // The destination committed, but the caller vanished before storing its ack.
        drop(owner);
        let mut reopened = open(directory.path());
        let mut state = SurfaceState::new();
        state.request(SurfaceAction::Control(Control::Reconcile));
        reopened.service(&mut state);
        assert!(state.error.is_none(), "{:?}", state.error);
        assert!(matches!(&state.view.unwrap().review,
            Some(Review::Added { receipt: held, .. }) if held == &receipt));
        assert_eq!(reopened.graph().nodes().count(), 4);
        assert_eq!(
            pollster::block_on(reopened.host.workshop_query(&acceptance, &proposal)).unwrap(),
            Some(receipt)
        );
    }

    #[test]
    fn workshop_graphshell_writer_lease_excludes_second_owner_and_releases_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let owner = open(directory.path());
        assert!(
            pollster::block_on(GraphshellWorkshop::open(directory.path(), Arc::new(|| {})))
                .is_err()
        );
        drop(owner);
        let _reopened = open(directory.path());
    }

    #[test]
    fn workshop_graphshell_result_route_does_not_substitute_another_surface() {
        let directory = tempfile::tempdir().unwrap();
        let mut owner = open(directory.path());
        let mut state = generated(&mut owner);
        state.request(SurfaceAction::Control(Control::Accept));
        owner.service(&mut state);
        assert!(state.error.is_none(), "{:?}", state.error);
        let Review::Added {
            acceptance,
            receipt,
        } = state.view.as_ref().unwrap().review.clone().unwrap()
        else {
            panic!("expected acknowledgement")
        };
        let original = athanor::workshop_destination::result_surface(&acceptance);
        let snapshot = owner.graph().to_snapshot_at(0);
        let url = snapshot
            .nodes
            .iter()
            .find(|n| n.node_id == original.to_string())
            .unwrap()
            .url
            .clone();
        let alias = Uuid::new_v4();
        pollster::block_on(owner.host.workshop_seed(vec![
            GraphDelta::AddNode {
                id: Some(alias),
                url,
                position: (0., 0.).into(),
            },
            GraphDelta::ReplaySetNodeBodyById {
                node_id: alias,
                body: Some("another surface's inline text".into()),
            },
            GraphDelta::ReplayRemoveNodeById { node_id: original },
        ]))
        .unwrap();
        state.reading = None;
        state.request(SurfaceAction::OpenResult(receipt.resource));
        owner.service(&mut state);
        assert!(state.reading.is_none());
        assert!(
            state
                .error
                .as_ref()
                .is_some_and(|error| error.contains("removed"))
        );
        assert!(
            pollster::block_on(owner.host.workshop_query(
                &acceptance,
                state.view.as_ref().unwrap().proposal.as_ref().unwrap()
            ))
            .unwrap()
            .is_some()
        );
    }

    #[test]
    fn workshop_graphshell_reject_creates_no_note_and_changed_source_refuses_acceptance() {
        let directory = tempfile::tempdir().unwrap();
        let mut owner = open(directory.path());
        let mut state = generated(&mut owner);
        state.request(SurfaceAction::Control(Control::Reject));
        owner.service(&mut state);
        assert!(state.error.is_none(), "{:?}", state.error);
        assert_eq!(owner.graph().nodes().count(), 3);
        let mut state = generated(&mut owner);
        let source = surface_id(owner.owner(), 1);
        pollster::block_on(
            owner
                .host
                .workshop_seed(vec![GraphDelta::ReplaySetNodeBodyById {
                    node_id: source,
                    body: Some("a changed version".into()),
                }]),
        )
        .unwrap();
        state.request(SurfaceAction::Control(Control::Accept));
        owner.service(&mut state);
        assert!(
            state
                .error
                .as_ref()
                .is_some_and(|e| e.contains("source version changed"))
        );
        assert_eq!(owner.graph().nodes().count(), 3);
        assert!(matches!(
            state.view.unwrap().review,
            Some(Review::AwaitingReview)
        ));
    }
}
