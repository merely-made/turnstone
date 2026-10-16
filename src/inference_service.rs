// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's decoder-free client for the resident inference route.
use armillary::Wake;
use chirograph::{
    CarrierRequestBody, CarrierResponseBody, IntentInvocation, IntentResult, PortableCardV1,
    ProjectionRequest,
};
use distillery::inference_protocol::{self as wire, InferenceRunsV1, RunV1};
use eidetic::models::recall::{PassageSelector, RecallSourceId, RetainedSourceText};
use graphshell_endpoint::door::{
    app_admission::{AppId, AppRouteId},
    app_client::AppBrokerClient,
};
use sceno::InstanceId;
use scenotime::{Revision, SceneEpoch};
use std::{
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use wire::recall::{
    ASK_PUBLISHED_INTENT, MAX_PUBLISHED_CORPUS_BYTES, PUBLISH_CORPUS_INTENT,
    PublishCorpusRequestV1, PublishedAskRequestV1,
};

/// Storage remains behind its one trail actor; only exact source data crosses this seam.
pub(crate) trait RetainedSourceOwner: Send + Sync {
    fn snapshot(&self, owner: &str) -> Result<Vec<RetainedSourceText>, String>;
    fn resolve(&self, source: &RecallSourceId, version: &str) -> Result<String, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenSource {
    pub run: String,
    pub retained: RetainedSourceText,
    pub selector: PassageSelector,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClientSnapshot {
    pub revision: u64,
    pub status: String,
    pub ready: bool,
    pub model: Option<String>,
    pub runs: Vec<RunV1>,
    pub source: Option<OpenSource>,
}
#[derive(Clone, Copy)]
pub(crate) enum Control {
    Pause,
    Resume,
    Cancel,
}
impl Control {
    fn intent(self) -> &'static str {
        match self {
            Self::Pause => wire::PAUSE_INTENT,
            Self::Resume => wire::RESUME_INTENT,
            Self::Cancel => wire::CANCEL_INTENT,
        }
    }
}
enum Command {
    Ask { run: String, question: String },
    Control { run: String, control: Control },
    Source { run: String, handle: u32 },
    Refresh,
}

pub(crate) struct InferenceService {
    commands: mpsc::SyncSender<Command>,
    snapshot: Arc<Mutex<ClientSnapshot>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    owner: String,
}
impl InferenceService {
    pub fn start(
        owner: String,
        sources: Arc<dyn RetainedSourceOwner>,
        wake: Wake,
    ) -> Result<Arc<Self>, String> {
        let (commands, rx) = mpsc::sync_channel(32);
        let snapshot = Arc::new(Mutex::new(ClientSnapshot {
            status: "Connecting to resident inference…".into(),
            ..Default::default()
        }));
        let state = snapshot.clone();
        let namespace = owner.clone();
        let worker = std::thread::Builder::new()
            .name("turnstone-inference".into())
            .spawn(move || worker(namespace, sources, rx, state, wake))
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(Self {
            commands,
            snapshot,
            worker: Mutex::new(Some(worker)),
            owner,
        }))
    }
    pub fn snapshot(&self) -> ClientSnapshot {
        self.snapshot.lock().unwrap().clone()
    }
    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|_| "The answer connection is busy or closed.".into())
    }
    pub fn ask(&self, question: String) -> Result<String, String> {
        if question.trim().is_empty() {
            return Err("Enter a question.".into());
        }
        let run = format!("{}/{}", self.owner, uuid::Uuid::new_v4());
        self.send(Command::Ask {
            run: run.clone(),
            question,
        })?;
        Ok(run)
    }
    pub fn control(&self, run: String, control: Control) -> Result<(), String> {
        self.send(Command::Control { run, control })
    }
    pub fn source(&self, run: String, handle: u32) -> Result<(), String> {
        self.send(Command::Source { run, handle })
    }
    pub fn refresh(&self) -> Result<(), String> {
        self.send(Command::Refresh)
    }
}
impl Drop for InferenceService {
    fn drop(&mut self) {
        // Close the sender before joining; every network operation has a timeout.
        let (closed, _) = mpsc::sync_channel(1);
        self.commands = closed;
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}
struct Connection {
    client: AppBrokerClient,
    request: ProjectionRequest,
    published: Option<eidetic::Hash>,
}
fn update(state: &Mutex<ClientSnapshot>, wake: &Wake, mut next: ClientSnapshot) {
    let mut current = state.lock().unwrap();
    next.revision = current.revision;
    if *current != next {
        next.revision += 1;
        *current = next;
        drop(current);
        wake();
    }
}
fn worker(
    owner: String,
    sources: Arc<dyn RetainedSourceOwner>,
    commands: mpsc::Receiver<Command>,
    snapshot: Arc<Mutex<ClientSnapshot>>,
    wake: Wake,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            update(
                &snapshot,
                &wake,
                ClientSnapshot {
                    status: format!("Answer connection could not start: {error}"),
                    ..Default::default()
                },
            );
            return;
        },
    };
    let mut connection = None;
    let mut next_connect = Instant::now();
    let mut next_sources = Instant::now();
    loop {
        let command = match commands.recv_timeout(wire::INFERENCE_ASK_NOTICE_POLL) {
            Ok(c) => Some(c),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if connection.is_none() && command.is_none() && Instant::now() < next_connect {
            continue;
        }
        let previous = snapshot.lock().unwrap().clone();
        let result=runtime.block_on(async {tokio::time::timeout(Duration::from_secs(5),async {
            if connection.is_none() {
                let mut client=AppBrokerClient::open_route(AppId::new("turnstone"),AppRouteId::new(wire::INFERENCE_ASK_ROUTE).map_err(|e|e.to_string())?).await.map_err(|e|e.to_string())?;
                let opened=client.open_session().await.map_err(|e|e.to_string())?;
                let request=opened.descriptor.projections.first().ok_or("The resident exposes no answer view.")?.request.clone();
                connection=Some(Connection {client,request,published:None});next_sources=Instant::now();
            }
            let connection=connection.as_mut().unwrap();
            let mut changed=false;
            if connection.published.is_none() || Instant::now()>=next_sources || matches!(&command,Some(Command::Ask {..})|Some(Command::Refresh)) {
                let retained=sources.snapshot(&owner)?;
                let publication=PublishCorpusRequestV1 {owner:owner.clone(),sources:retained};
                let bytes=serde_json::to_vec(&publication).map_err(|e|e.to_string())?;
                if bytes.len()>MAX_PUBLISHED_CORPUS_BYTES {return Err("Retained text is too large for this connection. Use a smaller browsing session.".into());}
                let fingerprint=eidetic::Hash::of(&bytes);
                if connection.published!=Some(fingerprint) {
                    intent(connection,PUBLISH_CORPUS_INTENT,bytes).await?;
                    connection.published=Some(fingerprint);changed=true;
                }
                next_sources=Instant::now()+Duration::from_secs(1);
            }
            let mut next=previous.clone();
            if let Some(command)=command {
                match command {
                    Command::Ask {run,question}=> {
                        intent(connection,ASK_PUBLISHED_INTENT,serde_json::to_vec(&PublishedAskRequestV1 {owner:owner.clone(),ask:wire::AskRequestV1 {run,question},scope:vec![]}).map_err(|e|e.to_string())?).await?;
                    },
                    Command::Control {run,control}=> {
                        if !run.starts_with(&format!("{owner}/")) {return Err("The answer belongs to another browsing session.".into());}
                        intent(connection,control.intent(),serde_json::to_vec(&wire::CancelRequestV1 {run}).map_err(|e|e.to_string())?).await?;
                    },
                    Command::Source {run,handle}=> {
                        // Refresh first so an old click cannot reopen a withdrawn citation.
                        next.runs=read_runs(connection).await?.runs.into_iter().filter(|r|r.run.starts_with(&format!("{owner}/"))).collect();
                        next.source=Some(resolve_source(sources.as_ref(),&next.runs,&run,handle)?);
                    },
                    Command::Refresh=>{},
                }
                changed=true;
            }
            if changed || connection.client.take_notice().await.map_err(|e|e.to_string())?.is_some() {
                let table=read_runs(connection).await?;
                next.ready=table.session.is_some() && table.load_error.is_none();next.model=table.model;
                next.status=table.load_error.unwrap_or_else(||if next.ready {"Connected to resident inference.".into()} else {"Loading the answer model…".into()});
                next.runs=table.runs.into_iter().filter(|r|r.run.starts_with(&format!("{owner}/"))).collect();
            }
            Ok::<_,String>(next)
        }).await.map_err(|_|"The answer connection timed out.".to_string())?});
        match result {
            Ok(next) => update(&snapshot, &wake, next),
            Err(error) => {
                connection = None;
                next_connect = Instant::now() + Duration::from_secs(2);
                update(
                    &snapshot,
                    &wake,
                    ClientSnapshot {
                        status: error,
                        ..Default::default()
                    },
                );
            },
        }
    }
    if let Some(connection) = connection {
        let _ = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(2), connection.client.close()).await
        });
    }
}
async fn intent(connection: &mut Connection, name: &str, payload: Vec<u8>) -> Result<(), String> {
    let result = connection
        .client
        .request_body(CarrierRequestBody::Intent(IntentInvocation {
            session: connection.request.session.clone(),
            target: InstanceId(0),
            observed_epoch: SceneEpoch(1),
            observed_revision: Revision(0),
            intent: name.into(),
            payload,
        }))
        .await
        .map_err(|e| e.to_string())?;
    match result {
        CarrierResponseBody::Intent(IntentResult::Accepted) => Ok(()),
        CarrierResponseBody::Intent(IntentResult::Rejected { reason }) => Err(reason),
        _ => Err("The resident returned an unexpected answer.".into()),
    }
}
async fn read_runs(connection: &mut Connection) -> Result<InferenceRunsV1, String> {
    let snapshot = connection
        .client
        .snapshot(connection.request.clone())
        .await
        .map_err(|e| e.to_string())?;
    let offer = snapshot
        .presentation
        .offers
        .values()
        .flatten()
        .next()
        .ok_or("The answer card is missing.")?;
    let bytes = connection
        .client
        .resource(snapshot.session.clone(), offer.resource)
        .await
        .map_err(|e| e.to_string())?;
    let card: PortableCardV1 = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let media = *card
        .media
        .first()
        .ok_or("The answer records are missing.")?;
    let bytes = connection
        .client
        .resource(snapshot.session, media)
        .await
        .map_err(|e| e.to_string())?;
    let table: InferenceRunsV1 = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if table.schema != wire::INFERENCE_RUNS_SCHEMA {
        return Err("The resident uses an unsupported answer format.".into());
    }
    Ok(table)
}
fn resolve_source(
    owner: &dyn RetainedSourceOwner,
    runs: &[RunV1],
    run: &str,
    handle: u32,
) -> Result<OpenSource, String> {
    let evidence = runs
        .iter()
        .find(|r| r.run == run)
        .and_then(|r| r.recall.as_ref())
        .ok_or("The retained answer is unavailable.")?;
    let citation = evidence
        .citations
        .iter()
        .find(|c| c.handle == handle)
        .ok_or("That citation is stale or unavailable.")?;
    let passage = &evidence
        .passages
        .iter()
        .find(|p| p.handle == handle)
        .ok_or("The cited passage is unavailable.")?
        .passage;
    if citation.passage != passage.id() {
        return Err("The citation does not name its supplied passage.".into());
    }
    let text = owner.resolve(&passage.source, &passage.version)?;
    if passage.selector.start > passage.selector.end || passage.selector.end > text.chars().count()
    {
        return Err("The cited source range is invalid.".into());
    }
    if eidetic::Hash::of(text.as_bytes()) != passage.text_hash {
        return Err("The retained source text no longer matches the cited version.".into());
    }
    let excerpt: String = text
        .chars()
        .skip(passage.selector.start)
        .take(passage.selector.end.saturating_sub(passage.selector.start))
        .collect();
    if excerpt != passage.text {
        return Err("The retained source range no longer matches the citation.".into());
    }
    Ok(OpenSource {
        run: run.into(),
        retained: RetainedSourceText {
            source: passage.source.clone(),
            version: passage.version.clone(),
            title: None,
            text,
        },
        selector: passage.selector.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use distillery::inference_protocol::{
        RunStateV1,
        recall::{Citation, CitationBasis, RecallEvidence, SuppliedPassage},
    };
    use eidetic::models::recall::{InferencePassage, RecallSourceKind};
    struct Source {
        text: String,
    }
    impl RetainedSourceOwner for Source {
        fn snapshot(&self, _: &str) -> Result<Vec<RetainedSourceText>, String> {
            unreachable!()
        }
        fn resolve(&self, _: &RecallSourceId, version: &str) -> Result<String, String> {
            if version != eidetic::Hash::of(self.text.as_bytes()).to_string() {
                return Err("wrong version".into());
            }
            Ok(self.text.clone())
        }
    }
    fn fixture() -> (Source, Vec<RunV1>) {
        let text = "Retained nebula source version: amberstone.".to_string();
        let passage = InferencePassage {
            source: RecallSourceId {
                kind: RecallSourceKind::PageText,
                owner: Some("turnstone/session".into()),
                source: "https://retained.test/note".into(),
                node: None,
            },
            version: eidetic::Hash::of(text.as_bytes()).to_string(),
            text_hash: eidetic::Hash::of(text.as_bytes()),
            selector: PassageSelector {
                start: 0,
                end: text.chars().count(),
            },
            chunker: "test/v1".into(),
            text: text.clone(),
        };
        let citation = Citation {
            handle: 1,
            passage: passage.id(),
            basis: CitationBasis::ModelMarker,
            answer_selector: PassageSelector { start: 0, end: 3 },
        };
        let run = RunV1 {
            run: "retained".into(),
            question: "nebula?".into(),
            state: RunStateV1::Finished,
            text: "[1]".into(),
            error: None,
            session: None,
            tokens: 1,
            continuations: vec![],
            recall: Some(RecallEvidence {
                owner: Some("turnstone/session".into()),
                scope: vec![],
                config: Default::default(),
                passages: vec![SuppliedPassage { handle: 1, passage }],
                stale: vec![],
                citations: vec![citation],
                unknown_citations: vec![],
            }),
        };
        (Source { text }, vec![run])
    }
    #[test]
    fn citation_open_reads_the_exact_source_version_and_checks_its_passage_identity() {
        let (owner, mut runs) = fixture();
        let opened = resolve_source(&owner, &runs, "retained", 1).unwrap();
        assert_eq!(opened.retained.text, owner.text);
        runs[0].recall.as_mut().unwrap().citations[0].passage =
            eidetic::ManifestId::of_blob(b"other passage");
        assert!(
            resolve_source(&owner, &runs, "retained", 1)
                .unwrap_err()
                .contains("does not name")
        );
    }
    #[test]
    fn withdrawn_citations_and_wrong_source_text_cannot_open() {
        let (mut owner, mut runs) = fixture();
        owner.text.push_str(" changed");
        assert!(resolve_source(&owner, &runs, "retained", 1).is_err());
        runs[0].recall.as_mut().unwrap().citations.clear();
        assert!(
            resolve_source(&owner, &runs, "retained", 1)
                .unwrap_err()
                .contains("stale or unavailable")
        );
    }
    #[test]
    fn a_selector_beyond_the_source_is_refused_even_when_it_would_copy_the_same_text() {
        let (owner, mut runs) = fixture();
        let evidence = runs[0].recall.as_mut().unwrap();
        evidence.passages[0].passage.selector.end += 1;
        evidence.citations[0].passage = evidence.passages[0].passage.id();
        assert!(
            resolve_source(&owner, &runs, "retained", 1)
                .unwrap_err()
                .contains("range is invalid")
        );
    }
}
