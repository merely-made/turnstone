// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Process-local behavior capture. Origin is supplied by each installed graph,
//! never recovered from focus or from the IDs a delta happens to name.
use crate::panes::{GraphId, SessionId};
use mere::kernel::graph::capture::{CapturedDelta, DeltaRecorder};
use mere::kernel::graph::{AttributedDelta, Author};
use std::ops::Deref;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeOrigin {
    pub graph: GraphId,
    pub session: Option<SessionId>,
    pub generation: u64,
}

impl RuntimeOrigin {
    pub fn is_current(self, session: SessionId, current: Option<Self>) -> bool {
        self.session == Some(session) && current == Some(self)
    }
}

/// A cascade temporarily owns its old table. A replacement session's table
/// must survive when that cascade returns, including same-session reloads.
pub fn restore_table<T>(
    destination: &mut T,
    old: T,
    expected: RuntimeOrigin,
    current: Option<RuntimeOrigin>,
) -> bool {
    if current != Some(expected) {
        return false;
    }
    *destination = old;
    true
}

#[derive(Clone, Debug)]
pub struct HostEntry {
    pub seq: u64,
    pub origin: RuntimeOrigin,
    pub edit: AttributedDelta,
}
impl Deref for HostEntry {
    type Target = AttributedDelta;
    fn deref(&self) -> &Self::Target {
        &self.edit
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalError {
    SequenceExhausted,
    GenerationExhausted,
    Poisoned,
}
impl std::fmt::Display for JournalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SequenceExhausted => "behavior journal sequence exhausted",
            Self::GenerationExhausted => "behavior runtime generation exhausted",
            Self::Poisoned => "behavior journal capture lock poisoned",
        })
    }
}
impl std::error::Error for JournalError {}

pub struct JournalTail {
    pub high_water: u64,
    pub entries: Vec<HostEntry>,
}

#[derive(Debug)]
pub struct HostJournal {
    entries: Vec<HostEntry>,
    author: Author,
    high_water: u64,
    generation: u64,
    error: Option<JournalError>,
}
impl Default for HostJournal {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            author: Author::user(),
            high_water: 0,
            generation: 0,
            error: None,
        }
    }
}
impl HostJournal {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn author(&self) -> &Author {
        &self.author
    }
    pub fn set_author(&mut self, author: Author) {
        self.author = author;
    }
    pub fn entries(&self) -> &[HostEntry] {
        &self.entries
    }
    pub fn high_water(&self) -> u64 {
        self.high_water
    }
    pub fn error(&self) -> Option<&JournalError> {
        self.error.as_ref()
    }
    pub fn execution_error(&self) -> Option<JournalError> {
        self.error
            .clone()
            .or_else(|| (self.high_water == u64::MAX).then_some(JournalError::SequenceExhausted))
    }

    pub fn allocate_origin(
        &mut self,
        graph: GraphId,
        session: Option<SessionId>,
    ) -> Result<RuntimeOrigin, JournalError> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let Some(generation) = self.generation.checked_add(1) else {
            self.error = Some(JournalError::GenerationExhausted);
            return Err(JournalError::GenerationExhausted);
        };
        self.generation = generation;
        Ok(RuntimeOrigin {
            graph,
            session,
            generation,
        })
    }

    /// A refused restored cursor must not disable an unrelated fresh session.
    pub fn raise_cursor_floor(&mut self, floor: u64) -> Result<(), JournalError> {
        if let Some(error) = self.execution_error() {
            return Err(error);
        }
        if floor == u64::MAX {
            return Err(JournalError::SequenceExhausted);
        }
        self.high_water = self.high_water.max(floor);
        Ok(())
    }

    pub fn record(
        &mut self,
        origin: RuntimeOrigin,
        delta: CapturedDelta,
    ) -> Result<u64, JournalError> {
        self.record_as(origin, self.author.clone(), delta)
    }
    pub fn record_as(
        &mut self,
        origin: RuntimeOrigin,
        author: Author,
        delta: CapturedDelta,
    ) -> Result<u64, JournalError> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let Some(seq) = self.high_water.checked_add(1) else {
            self.error = Some(JournalError::SequenceExhausted);
            return Err(JournalError::SequenceExhausted);
        };
        self.high_water = seq;
        self.entries.push(HostEntry {
            seq,
            origin,
            edit: AttributedDelta { author, delta },
        });
        Ok(seq)
    }
    /// Clone under one lock: the boundary includes foreign entries, while the
    /// payload contains only entries for this exact accepted graph lifetime.
    pub fn tail(&self, origin: RuntimeOrigin, cursor: u64) -> JournalTail {
        JournalTail {
            high_water: self.high_water,
            entries: self
                .entries
                .iter()
                .filter(|entry| entry.seq > cursor && entry.origin == origin)
                .cloned()
                .collect(),
        }
    }
}

pub type SharedJournal = Arc<Mutex<HostJournal>>;
pub fn shared_journal() -> SharedJournal {
    Arc::new(Mutex::new(HostJournal::new()))
}

pub fn recorder(journal: SharedJournal, origin: RuntimeOrigin) -> DeltaRecorder {
    Arc::new(move |delta| match journal.lock() {
        Ok(mut sink) => {
            let _ = sink.record(origin, delta.clone());
        },
        Err(poisoned) => {
            poisoned.into_inner().error = Some(JournalError::Poisoned);
        },
    })
}
/// Exact typed attribution survives nested synchronous lowering and unwind.
pub struct AuthorRestore {
    journal: SharedJournal,
    previous: Author,
}
impl AuthorRestore {
    pub fn enter(journal: SharedJournal, author: Author) -> Self {
        let previous = {
            let mut sink = journal
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = sink.author().clone();
            sink.set_author(author);
            previous
        };
        Self { journal, previous }
    }
}
impl Drop for AuthorRestore {
    fn drop(&mut self) {
        self.journal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_author(self.previous.clone());
    }
}

#[cfg(test)]
mod journal_tests {
    use super::*;
    use mere::kernel::graph::{Author, capture::CapturedDelta};
    fn graph(n: u128) -> GraphId {
        GraphId::from_uuid(uuid::Uuid::from_u128(n))
    }
    fn delta() -> CapturedDelta {
        CapturedDelta::ReplaySetNodeTitleById {
            node_id: uuid::Uuid::from_u128(7).to_string(),
            title: "changed".into(),
        }
    }
    #[test]
    fn equal_node_ids_keep_distinct_runtime_origins() {
        let mut j = HostJournal::new();
        let a = j.allocate_origin(graph(1), None).unwrap();
        let b = j.allocate_origin(graph(2), None).unwrap();
        assert_eq!(j.record_as(a, Author::user(), delta()).unwrap(), 1);
        assert_eq!(j.record_as(b, Author::user(), delta()).unwrap(), 2);
        assert_eq!(j.tail(a, 0).entries.len(), 1);
        assert_eq!(j.tail(b, 0).entries.len(), 1);
        assert_ne!(a, b);
    }
    #[test]
    fn restored_cursor_is_a_floor_not_a_vector_index() {
        let mut j = HostJournal::new();
        j.raise_cursor_floor(900).unwrap();
        let a = j.allocate_origin(graph(1), None).unwrap();
        assert_eq!(j.record_as(a, Author::user(), delta()).unwrap(), 901);
        assert_eq!(j.entries().len(), 1);
        assert_eq!(j.high_water(), 901);
        assert_eq!(j.tail(a, 900).entries[0].seq, 901);
        j.raise_cursor_floor(100).unwrap();
        assert_eq!(j.record_as(a, Author::user(), delta()).unwrap(), 902);
    }
    #[test]
    fn foreign_only_tail_still_advances_scan_boundary() {
        let mut j = HostJournal::new();
        let a = j.allocate_origin(graph(1), None).unwrap();
        let b = j.allocate_origin(graph(2), None).unwrap();
        j.record_as(b, Author::user(), delta()).unwrap();
        let tail = j.tail(a, 0);
        assert!(tail.entries.is_empty());
        assert_eq!(tail.high_water, 1);
        assert!(j.tail(a, tail.high_water).entries.is_empty());
    }
    #[test]
    fn reloaded_graph_and_session_get_new_generation() {
        let mut j = HostJournal::new();
        let session = Some(SessionId::new());
        let old = j.allocate_origin(graph(1), session).unwrap();
        j.record_as(old, Author::user(), delta()).unwrap();
        let new = j.allocate_origin(graph(1), session).unwrap();
        assert_ne!(old, new);
        assert!(j.tail(new, 0).entries.is_empty());
    }
    #[test]
    fn exhausted_restored_cursor_refuses_without_poisoning_other_sessions() {
        let mut j = HostJournal::new();
        assert_eq!(
            j.raise_cursor_floor(u64::MAX),
            Err(JournalError::SequenceExhausted)
        );
        assert_eq!(j.high_water(), 0);
        let a = j.allocate_origin(graph(1), None).unwrap();
        assert_eq!(j.record_as(a, Author::user(), delta()).unwrap(), 1);
    }
    #[test]
    fn append_exhaustion_is_latched_without_wrapping() {
        let mut j = HostJournal::new();
        let a = j.allocate_origin(graph(1), None).unwrap();
        j.raise_cursor_floor(u64::MAX - 1).unwrap();
        assert_eq!(j.record_as(a, Author::user(), delta()).unwrap(), u64::MAX);
        assert_eq!(
            j.record_as(a, Author::user(), delta()),
            Err(JournalError::SequenceExhausted)
        );
        assert_eq!(j.high_water(), u64::MAX);
        assert_eq!(j.entries().len(), 1);
        assert_eq!(j.error(), Some(&JournalError::SequenceExhausted));
    }
    #[test]
    fn complete_typed_author_is_retained() {
        let mut j = HostJournal::new();
        let a = j.allocate_origin(graph(1), None).unwrap();
        let author = Author::script("subject", "revision").via("turnstone");
        j.set_author(author.clone());
        j.record(a, delta()).unwrap();
        assert_eq!(j.entries()[0].edit.author, author);
        assert_eq!(j.entries()[0].origin, a);
    }
    #[test]
    fn graph_owned_recorder_records_once_and_clone_scratch_is_silent() {
        use mere::kernel::graph::{
            Graph,
            apply::{GraphDelta, add_node, apply_graph_delta},
        };
        let journal = shared_journal();
        let origin = journal
            .lock()
            .unwrap()
            .allocate_origin(graph(1), None)
            .unwrap();
        let mut live = Graph::new();
        live.set_recorder(Some(recorder(journal.clone(), origin)));
        let key = add_node(
            &mut live,
            Some(uuid::Uuid::from_u128(7)),
            "https://test/".into(),
            Default::default(),
        );
        let before = journal.lock().unwrap().entries().len();
        apply_graph_delta(
            &mut live,
            GraphDelta::SetNodeTitle {
                key,
                title: "live title".into(),
            },
        );
        assert_eq!(journal.lock().unwrap().entries().len(), before + 1);
        assert_eq!(
            journal.lock().unwrap().entries().last().unwrap().origin,
            origin
        );
        let mut scratch = live.clone();
        assert!(!scratch.is_recording());
        apply_graph_delta(
            &mut scratch,
            GraphDelta::SetNodeTitle {
                key,
                title: "scratch title".into(),
            },
        );
        assert_eq!(journal.lock().unwrap().entries().len(), before + 1);
    }
    #[test]
    fn nested_author_guards_restore_all_fields_even_on_unwind() {
        let journal = shared_journal();
        let prior = Author::engine("previous", "engine-1").via("route-1");
        let outer = Author::script("outer", "body-1").via("turnstone");
        let inner = Author::script("inner", "body-2").via("endpoint");
        journal.lock().unwrap().set_author(prior.clone());
        {
            let _outer = AuthorRestore::enter(journal.clone(), outer.clone());
            let result = std::panic::catch_unwind(|| {
                let _inner = AuthorRestore::enter(journal.clone(), inner.clone());
                assert_eq!(journal.lock().unwrap().author(), &inner);
                panic!("script failed");
            });
            assert!(result.is_err());
            assert_eq!(journal.lock().unwrap().author(), &outer);
        }
        assert_eq!(journal.lock().unwrap().author(), &prior);
    }
    #[test]
    fn generation_exhaustion_refuses_and_latches() {
        let mut journal = HostJournal::new();
        journal.generation = u64::MAX;
        assert_eq!(
            journal.allocate_origin(graph(1), None),
            Err(JournalError::GenerationExhausted)
        );
        assert_eq!(journal.error(), Some(&JournalError::GenerationExhausted));
    }
    #[test]
    fn poisoned_capture_is_visible_and_cannot_supply_fresh_triggers() {
        let journal = shared_journal();
        let origin = journal
            .lock()
            .unwrap()
            .allocate_origin(graph(1), None)
            .unwrap();
        let capture = recorder(journal.clone(), origin);
        let sink = journal.clone();
        assert!(
            std::panic::catch_unwind(move || {
                let _locked = sink.lock().unwrap();
                panic!("bad writer");
            })
            .is_err()
        );
        capture(&delta());
        let poisoned = journal.lock().unwrap_err().into_inner();
        assert_eq!(poisoned.error(), Some(&JournalError::Poisoned));
        assert!(poisoned.entries().is_empty());
    }
    #[test]
    fn typed_subject_projection_preserves_no_self_wake() {
        use servitor::{ScopePath, Subject, Watch};
        let subject = Subject::new([0x31; 32]);
        let mut journal = HostJournal::new();
        let origin = journal.allocate_origin(graph(1), None).unwrap();
        let author = Author::script(subject.to_hex(), "body-1").via("turnstone");
        journal.record_as(origin, Author::user(), delta()).unwrap();
        journal.record_as(origin, author.clone(), delta()).unwrap();
        let scope = ScopePath::parse(&uuid::Uuid::from_u128(7).to_string()).unwrap();
        let watch = Watch {
            subject,
            scope: scope.clone(),
            self_author: subject.to_hex(),
            cursor: 0,
        };
        let scopes = [scope];
        let entries = journal.tail(origin, 0).entries;
        assert!(watch.matches(&servitor::WatchEvent {
            seq: entries[0].seq,
            author: &entries[0].author.id,
            scopes: &scopes
        }));
        assert!(!watch.matches(&servitor::WatchEvent {
            seq: entries[1].seq,
            author: &entries[1].author.id,
            scopes: &scopes
        }));
        assert_eq!(entries[1].author, author);
    }

    #[test]
    fn runtime_binding_requires_exact_generation_and_session() {
        let mut journal = HostJournal::new();
        let session = SessionId::new();
        let origin = journal.allocate_origin(graph(1), Some(session)).unwrap();
        assert!(origin.is_current(session, Some(origin)));
        assert!(!origin.is_current(SessionId::new(), Some(origin)));
        let replacement = journal.allocate_origin(graph(1), Some(session)).unwrap();
        assert!(!origin.is_current(session, Some(replacement)));
        assert!(!origin.is_current(session, None));
        let detached = journal.allocate_origin(graph(1), None).unwrap();
        assert!(!detached.is_current(session, Some(detached)));
    }
    #[test]
    fn interrupted_cascade_keeps_new_watch_table_after_same_session_reload() {
        use servitor::{Subject, WatchTable};
        let mut journal = HostJournal::new();
        let session = SessionId::new();
        let old_origin = journal.allocate_origin(graph(1), Some(session)).unwrap();
        let new_origin = journal.allocate_origin(graph(1), Some(session)).unwrap();
        let table = |byte| {
            let line = format!("{} 123 user /", Subject::new([byte; 32]).to_hex());
            WatchTable::from_wire_lines([line.as_str()]).unwrap()
        };
        let mut current = table(2);
        let expected = current.to_wire_lines();
        assert!(!restore_table(
            &mut current,
            table(1),
            old_origin,
            Some(new_origin)
        ));
        assert_eq!(current.to_wire_lines(), expected);
        let old = table(1);
        let expected = old.to_wire_lines();
        assert!(restore_table(
            &mut current,
            old,
            new_origin,
            Some(new_origin)
        ));
        assert_eq!(current.to_wire_lines(), expected);
    }
    #[test]
    fn final_captured_ordinal_refuses_the_next_execution_before_any_write() {
        let mut journal = HostJournal::new();
        let origin = journal.allocate_origin(graph(1), None).unwrap();
        journal.raise_cursor_floor(u64::MAX - 1).unwrap();
        journal.record_as(origin, Author::user(), delta()).unwrap();
        assert!(
            journal.error().is_none(),
            "the final entry itself was fully captured"
        );
        assert_eq!(
            journal.execution_error(),
            Some(JournalError::SequenceExhausted)
        );
        assert_eq!(journal.entries().len(), 1);
    }
}
