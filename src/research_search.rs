// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Model-free owner adapter for one frozen admitted search.
use crate::research::{self, ClipAdmission, Context, Target};
use eidetic::models::recall::RetainedSourceText;
use eidetic_search::query::{
    self, Coverage, ProducerKind, ResearchSnapshot, ResearchSource, SearchQuery, SearchResult,
    SearchSettings, TagId,
};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Member {
    Capture(crate::saved_sources::Key),
    Clip(ClipAdmission),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: uuid::Uuid,
    pub context: Context,
    pub revision: u64,
    pub epoch: u64,
    pub query_revision: u64,
    pub selected: Option<Vec<Member>>,
    pub query: SearchQuery,
    pub settings: SearchSettings,
}
pub(crate) struct Response {
    pub request: Request,
    pub result: Result<SearchResult, crate::research_storage::SourceReadError>,
}

pub(crate) fn producer(view: &crate::retained_clip::ClipView) -> ProducerKind {
    let crate::retained_clip::ClipBody::Text(text) = &view.body else {
        return ProducerKind::Other;
    };
    let parts: Vec<_> = view
        .clip
        .parts
        .iter()
        .filter(|part| part.name == "body")
        .collect();
    if parts.len() != 1 || parts[0].media_type != "text/plain;charset=utf-8" {
        return ProducerKind::Other;
    }
    let evidence = &view.clip.source_evidence;
    let knot_anchor_ok = view.clip.anchors.iter().all(|anchor| {
        let quote = anchor.selector.get("exact_quote").and_then(|v| v.as_str());
        anchor.units == "utf8-bytes"
            && quote.is_some_and(|quote| !quote.is_empty())
            && anchor
                .selector
                .get("document_address")
                .and_then(|v| v.as_str())
                == Some(anchor.source.as_str())
            && anchor
                .selector
                .get("byte_span")
                .and_then(|span| {
                    span.get("start")
                        .and_then(|v| v.as_u64())
                        .zip(span.get("end").and_then(|v| v.as_u64()))
                })
                .is_some_and(|(start, end)| {
                    end.checked_sub(start)
                        == quote.and_then(|quote| u64::try_from(quote.len()).ok())
                })
            && anchor.selector.get("source_hash").and_then(|v| v.as_str())
                == anchor.representation.as_deref()
    });
    if parts[0].role == "selected-body"
        && matches!(
            evidence.get("kind").and_then(|v| v.as_str()),
            Some("word" | "passage" | "sense" | "pronunciation" | "note")
        )
        && evidence
            .get("source_tags")
            .and_then(|v| v.as_array())
            .is_some_and(|tags| tags.iter().all(|tag| tag.as_str().is_some()))
        && evidence
            .get("pack_sources")
            .is_some_and(|v| v.is_null() || v.is_array())
        && evidence.get("tag_asserter").is_some_and(|v| v.is_null())
        && knot_anchor_ok
    {
        return ProducerKind::Selection;
    }
    let note_anchor_ok = view.clip.anchors.len() == 1
        && view.clip.anchors.iter().all(|anchor| {
            let body = &anchor.evidence["body"];
            anchor.units == "milliseconds"
                && !anchor.source.trim().is_empty()
                && anchor
                    .selector
                    .get("offset_ms")
                    .and_then(|v| v.as_u64())
                    .is_some()
                && body.get("kind").and_then(|v| v.as_str()) == Some("text")
                && body.get("plain_text").and_then(|v| v.as_str()) == Some(text.as_str())
                && anchor
                    .evidence
                    .get("source_revision")
                    .is_some_and(|v| v.is_null())
                && anchor
                    .evidence
                    .get("historical_author")
                    .is_some_and(|v| v.is_null())
        });
    if parts[0].role == "note-body" && evidence.is_null() && note_anchor_ok {
        return ProducerKind::Note;
    }
    ProducerKind::Other
}

fn add_cost(total: &mut usize, value: &str) -> Result<(), String> {
    *total = total
        .checked_add(value.chars().count().max(1))
        .ok_or("metadata size overflow")?;
    Ok(())
}
fn id_cost(total: &mut usize, id: &eidetic::models::recall::RecallSourceId) -> Result<(), String> {
    add_cost(total, &id.source)?;
    if let Some(value) = &id.owner {
        add_cost(total, value)?;
    }
    if let Some(value) = &id.node {
        add_cost(total, value)?;
    }
    Ok(())
}
pub(crate) fn source_metadata_cost(source: &ResearchSource) -> Result<usize, String> {
    let mut total = 0;
    id_cost(&mut total, &source.retained.source)?;
    id_cost(&mut total, &source.retained.source)?; // scope plus source
    add_cost(&mut total, &source.retained.version)?;
    if let Some(title) = &source.retained.title {
        add_cost(&mut total, title)?;
    }
    if let Some(address) = &source.address {
        add_cost(&mut total, address)?;
    }
    for tag in &source.tags {
        add_cost(&mut total, &tag.owner)?;
        add_cost(&mut total, &tag.concept)?;
    }
    Ok(total)
}
pub(crate) fn capture_metadata_cost(entry: &crate::saved_sources::Entry) -> Result<usize, String> {
    let Some(content) = &entry.content else {
        let exclusion = format!(
            "{}: {}",
            entry.admission.title,
            entry.error.as_deref().unwrap_or("unavailable")
        );
        return Ok(exclusion.chars().count().max(1));
    };
    let target = Target::Capture(crate::saved_sources::Target {
        key: entry.admission.key.clone(),
        text_hash: content.text_hash.clone(),
        selector: None,
    });
    let source = ResearchSource {
        retained: RetainedSourceText {
            source: research::source_id(&target)?,
            version: content.text_hash.clone(),
            title: Some(content.title.clone()),
            text: String::new(),
        },
        producer: ProducerKind::Capture,
        address: Some(content.origin.clone()),
        date: None,
        tags: vec![],
    };
    source_metadata_cost(&source)
}

pub(crate) fn snapshot(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<crate::trail_memory::TrailCommand>,
    request: &Request,
) -> Result<ResearchSnapshot, crate::research_storage::SourceReadError> {
    snapshot_with_budget(bridge, trail, request, bridge.storage_limits()?.budget())
}

pub(crate) fn snapshot_with_budget(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<crate::trail_memory::TrailCommand>,
    request: &Request,
    budget: crate::research_storage::SharedReadBudget,
) -> Result<ResearchSnapshot, crate::research_storage::SourceReadError> {
    if bridge.context_revision()? != (request.context.clone(), request.revision) {
        return Err("Search admissions changed.".into());
    }
    let access = bridge.clip_access()?;
    let epoch = (access.epoch)().ok_or_else(|| crate::identity::PENDING.to_string())?;
    if epoch != request.epoch {
        return Err("Search custody changed.".into());
    }
    let selected = request.selected.as_ref();
    if selected.is_some_and(|members| {
        members.iter().any(|member| match member {
            Member::Capture(key) => !request.context.admissions.iter().any(|a| &a.key == key),
            Member::Clip(clip) => !request.context.clips.contains(clip),
        })
    }) {
        return Err("A chosen source is no longer admitted.".into());
    }
    let count = match selected {
        Some(members) => members.len(),
        None => request
            .context
            .admissions
            .iter()
            .map(|a| &a.key)
            .collect::<std::collections::HashSet<_>>()
            .len()
            .saturating_add(request.context.clips.len()),
    };
    if request.settings.max_sources == 0 || count > request.settings.max_sources {
        return Err("source limit exceeded".into());
    }
    let keys = selected.map(|members| {
        members
            .iter()
            .filter_map(|member| match member {
                Member::Capture(key) => Some(key.clone()),
                _ => None,
            })
            .collect()
    });
    let generation = format!("{}:{}", request.context.generation, request.revision);
    let mut metadata_chars = generation.chars().count().max(1);
    if metadata_chars > request.settings.max_metadata_chars {
        return Err("metadata limit exceeded".into());
    }
    let (reply, receive) = mpsc::sync_channel(1);
    if !trail.command(crate::trail_memory::TrailCommand::ResearchSnapshot {
        context: request.context.clone(),
        selected: keys,
        max_chars: Some(request.settings.max_corpus_chars),
        max_metadata_chars: Some(request.settings.max_metadata_chars - metadata_chars),
        budget: Some(budget.clone()),
        reply,
    }) {
        return Err("The saved-source owner is closed.".into());
    }
    let entries = receive
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| "The saved-source owner did not answer.")??;
    let mut sources = Vec::new();
    let mut exclusions = Vec::new();
    for entry in entries {
        metadata_chars = metadata_chars
            .checked_add(capture_metadata_cost(&entry)?)
            .ok_or("metadata size overflow")?;
        if metadata_chars > request.settings.max_metadata_chars {
            return Err("metadata limit exceeded".into());
        }
        if let Some(content) = entry.content {
            let target = Target::Capture(crate::saved_sources::Target {
                key: entry.admission.key,
                text_hash: content.text_hash.clone(),
                selector: None,
            });
            let source = research::source_id(&target)?;
            if sources
                .iter()
                .any(|s: &ResearchSource| s.retained.source == source)
            {
                continue;
            }
            sources.push(ResearchSource {
                retained: RetainedSourceText {
                    source,
                    version: content.text_hash,
                    title: Some(content.title),
                    text: content.text,
                },
                producer: ProducerKind::Capture,
                address: Some(content.origin),
                date: i64::try_from(content.captured_at_ms).ok(),
                tags: vec![],
            });
        } else {
            exclusions.push(format!(
                "{}: {}",
                entry.admission.title,
                entry.error.unwrap_or_else(|| "unavailable".into())
            ));
        }
    }
    let mut total_chars = sources
        .iter()
        .try_fold(0usize, |total, source| {
            total.checked_add(source.retained.text.chars().count())
        })
        .ok_or("corpus size overflow")?;
    for admission in &request.context.clips {
        if selected.is_some_and(|members| !members.contains(&Member::Clip(admission.clone()))) {
            continue;
        }
        match bridge.clip_view_bounded(&request.context, admission, budget.clone()) {
            Ok(view) => {
                let kind = producer(&view);
                let tags = view
                    .clip
                    .tags
                    .iter()
                    .map(|tag| TagId {
                        owner: tag.owner.clone(),
                        concept: tag.concept.clone(),
                    })
                    .collect();
                let address = (!view.clip.source.entity.trim().is_empty())
                    .then(|| view.clip.source.entity.clone());
                let crate::retained_clip::ClipBody::Text(text) = view.body else {
                    unreachable!()
                };
                total_chars = total_chars
                    .checked_add(text.chars().count())
                    .ok_or("corpus size overflow")?;
                if total_chars > request.settings.max_corpus_chars {
                    return Err("corpus limit exceeded while loading retained clips".into());
                }
                let hash = eidetic::Hash::of(text.as_bytes()).to_string();
                let target = Target::Clip(research::ClipTarget {
                    clip: admission.clone(),
                    text_hash: hash.clone(),
                    selector: None,
                });
                let source = ResearchSource {
                    retained: RetainedSourceText {
                        source: research::source_id(&target)?,
                        version: hash,
                        title: Some(view.clip.title),
                        text,
                    },
                    producer: kind,
                    address,
                    date: None,
                    tags,
                };
                metadata_chars = metadata_chars
                    .checked_add(source_metadata_cost(&source)?)
                    .ok_or("metadata size overflow")?;
                if metadata_chars > request.settings.max_metadata_chars {
                    return Err("metadata limit exceeded while loading retained clips".into());
                }
                sources.push(source);
            },
            Err(error) if error.storage_error().is_some() => {
                return Err(error);
            },
            Err(error) => {
                let exclusion = format!("{}: {error}", admission.manifest);
                add_cost(&mut metadata_chars, &exclusion)?;
                if metadata_chars > request.settings.max_metadata_chars {
                    return Err("metadata limit exceeded while loading retained clips".into());
                }
                exclusions.push(exclusion);
            },
        }
    }
    if (access.epoch)() != Some(epoch)
        || bridge.context_revision()? != (request.context.clone(), request.revision)
    {
        return Err("Search custody or admissions changed.".into());
    }
    let scope = sources
        .iter()
        .map(|source| source.retained.source.clone())
        .collect();
    let snapshot = ResearchSnapshot {
        generation,
        scope,
        sources,
        coverage: if exclusions.is_empty() {
            Coverage::Complete
        } else {
            Coverage::Incomplete { exclusions }
        },
    };
    Ok(snapshot)
}

pub(crate) fn run(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<crate::trail_memory::TrailCommand>,
    request: &Request,
) -> Result<SearchResult, crate::research_storage::SourceReadError> {
    let snapshot = snapshot(bridge, trail, request)?;
    query::search(&snapshot, &request.query, &request.settings).map_err(Into::into)
}

/// One running job, one replaceable pending request and one replaceable response.
enum Job {
    Search(Request),
    Compare(crate::research_compare::Request),
    Records(crate::research_records::Job),
}
enum Completed {
    Search(Response),
    Compare(crate::research_compare::Response),
    Records(crate::research_records::JobResult),
}
struct Queue {
    request: Option<Job>,
    response: Option<Completed>,
    serial: u64,
    closed: bool,
}
pub(crate) struct Worker {
    pending: Arc<(Mutex<Queue>, Condvar)>,
}
impl Worker {
    pub fn spawn(
        bridge: research::Bridge,
        trail: Arc<armillary::ActorHandle<crate::trail_memory::TrailCommand>>,
        wake: armillary::Wake,
    ) -> Self {
        let pending = Arc::new((
            Mutex::new(Queue {
                request: None,
                response: None,
                serial: 0,
                closed: false,
            }),
            Condvar::new(),
        ));
        let queue = pending.clone();
        std::thread::Builder::new()
            .name("research-search".into())
            .spawn(move || {
                loop {
                    let (lock, ready) = &*queue;
                    let mut slot = lock.lock().unwrap();
                    while slot.request.is_none() && !slot.closed {
                        slot = ready.wait(slot).unwrap();
                    }
                    if slot.closed {
                        break;
                    }
                    let request = slot.request.take().unwrap();
                    let serial = slot.serial;
                    drop(slot);
                    let response = match request {
                        Job::Search(request) => {
                            let result = run(&bridge, &trail, &request);
                            Completed::Search(Response { request, result })
                        },
                        Job::Compare(request) => {
                            let result = crate::research_compare::run(&bridge, &trail, &request);
                            Completed::Compare(crate::research_compare::Response {
                                request,
                                result,
                            })
                        },
                        Job::Records(request) => Completed::Records(
                            crate::research_records_worker::execute(&bridge, &trail, request),
                        ),
                    };
                    let mut slot = lock.lock().unwrap();
                    if !slot.closed && slot.serial == serial {
                        slot.response = Some(response);
                        drop(slot);
                        wake();
                    }
                }
            })
            .expect("spawn research search worker");
        Self { pending }
    }
    pub fn submit(&self, request: Request) {
        let (lock, ready) = &*self.pending;
        let mut queue = lock.lock().unwrap();
        queue.serial = queue.serial.wrapping_add(1);
        queue.response = None;
        queue.request = Some(Job::Search(request));
        ready.notify_one();
    }
    pub fn take(&self) -> Option<Response> {
        {
            let mut queue = self.pending.0.lock().unwrap();
            if matches!(queue.response, Some(Completed::Search(_))) {
                if let Some(Completed::Search(response)) = queue.response.take() {
                    return Some(response);
                }
            }
            None
        }
    }
    pub fn submit_compare(&self, request: crate::research_compare::Request) {
        let (lock, ready) = &*self.pending;
        let mut queue = lock.lock().unwrap();
        queue.serial = queue.serial.wrapping_add(1);
        queue.response = None;
        queue.request = Some(Job::Compare(request));
        ready.notify_one();
    }
    pub fn submit_records(&self, request: crate::research_records::Job) {
        let (lock, ready) = &*self.pending;
        let mut queue = lock.lock().unwrap();
        queue.serial = queue.serial.wrapping_add(1);
        queue.response = None;
        queue.request = Some(Job::Records(request));
        ready.notify_one();
    }
    pub fn take_records(&self) -> Option<crate::research_records::JobResult> {
        let mut queue = self.pending.0.lock().unwrap();
        if matches!(queue.response, Some(Completed::Records(_))) {
            if let Some(Completed::Records(response)) = queue.response.take() {
                return Some(response);
            }
        }
        None
    }
    pub fn take_compare(&self) -> Option<crate::research_compare::Response> {
        let mut queue = self.pending.0.lock().unwrap();
        if matches!(queue.response, Some(Completed::Compare(_))) {
            if let Some(Completed::Compare(response)) = queue.response.take() {
                return Some(response);
            }
        }
        None
    }
    pub fn cancel(&self) {
        let mut queue = self.pending.0.lock().unwrap();
        queue.serial = queue.serial.wrapping_add(1);
        queue.request = None;
        queue.response = None;
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, ready) = &*self.pending;
        let mut queue = lock.lock().unwrap();
        queue.closed = true;
        queue.request = None;
        queue.response = None;
        ready.notify_one();
    }
}
