// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Bounded, explicit structured research requests and owner adapter.
use crate::research::{Context, Target};
use crate::research_search::Member;
use eidetic::records::{
    PropertyDefinitionV1, PropertyRecordV1, PropertyValue, RecordQuery, RecordResult,
    RecordSettings, TimeInterval,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeToken {
    pub context: Context,
    pub revision: u64,
    pub custody_epoch: u64,
    pub graphs: Vec<(crate::panes::GraphId, u64)>,
    pub graph_instances: Vec<(crate::panes::GraphId, uuid::Uuid)>,
    pub selected: Option<Vec<Member>>,
    pub(crate) graph_manifests: Vec<(crate::panes::GraphId, eidetic::ManifestId)>,
    pub(crate) owner_bindings: Vec<OwnerBinding>,
    pub(crate) settings: RecordSettings,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnerBinding {
    pub binding: Binding,
    pub author: String,
    pub withdrawn: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub graph: crate::panes::GraphId,
    pub entity: uuid::Uuid,
    pub statement_id: String,
    pub manifest: eidetic::ManifestId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub binding: Binding,
    pub author: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRequest {
    pub id: uuid::Uuid,
    pub scope: ScopeToken,
    pub selected: Option<Vec<Member>>,
    pub query: RecordQuery,
    pub settings: RecordSettings,
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) result_anchor: Option<(crate::panes::GraphId, uuid::Uuid)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManualProperty {
    pub graph: crate::panes::GraphId,
    pub entity: uuid::Uuid,
    pub definition: PropertyDefinitionV1,
    pub value: PropertyValue,
    pub context: String,
    pub valid_time: Option<TimeInterval>,
    pub status: Option<String>,
    pub evidence: Target,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorRequest {
    pub id: uuid::Uuid,
    pub scope: ScopeToken,
    pub selected: Option<Vec<Member>>,
    pub property: ManualProperty,
    pub(crate) author: String,
    pub(crate) recorded_at: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    Query(QueryRequest),
    Deposit(AuthorRequest),
    Inspect {
        id: uuid::Uuid,
        scope: ScopeToken,
        binding: Binding,
    },
    Replay {
        id: uuid::Uuid,
        scope: ScopeToken,
        result: eidetic::ManifestId,
    },
    ConfirmProperty {
        request: AuthorRequest,
        scope: ScopeToken,
        binding: Binding,
    },
    ConfirmResult {
        request: QueryRequest,
        scope: ScopeToken,
        binding: Binding,
        value: RecordResult,
    },
    MarkProperty {
        request: AuthorRequest,
        scope: ScopeToken,
        binding: Binding,
        assertion: serde_json::Value,
    },
    MarkResult {
        request: QueryRequest,
        scope: ScopeToken,
        binding: Binding,
        assertion: serde_json::Value,
        value: RecordResult,
    },
    PrepareWithdrawal {
        id: uuid::Uuid,
        scope: ScopeToken,
        binding: Binding,
        nonce: uuid::Uuid,
    },
    ConfirmWithdrawal {
        id: uuid::Uuid,
        scope: ScopeToken,
        binding: Binding,
        nonce: uuid::Uuid,
    },
    Recovery {
        id: uuid::Uuid,
        scope: ScopeToken,
    },
    Reconcile {
        id: uuid::Uuid,
        scope: ScopeToken,
        item: crate::trail_memory::record_actor::RecoveryItem,
    },
    RecoveryMark {
        id: uuid::Uuid,
        scope: ScopeToken,
        item: crate::trail_memory::record_actor::RecoveryItem,
        binding: Binding,
        assertion: serde_json::Value,
    },
    RecoveryAck {
        id: uuid::Uuid,
        scope: ScopeToken,
        item: crate::trail_memory::record_actor::RecoveryItem,
        binding: Binding,
    },
    Abandon {
        id: uuid::Uuid,
        scope: ScopeToken,
        nonce: uuid::Uuid,
    },
    Discover {
        id: uuid::Uuid,
        scope: ScopeToken,
        bindings: Vec<Binding>,
        history: bool,
    },
    OpenSource {
        id: uuid::Uuid,
        scope: ScopeToken,
        generation: uuid::Uuid,
        source: eidetic::models::recall::RecallSourceId,
        version: String,
    },
    OpenEvidence {
        id: uuid::Uuid,
        scope: ScopeToken,
        generation: Option<uuid::Uuid>,
        binding: Binding,
        record: PropertyRecordV1,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobResult {
    Query {
        request: QueryRequest,
        result: Result<RecordResult, crate::research_storage::SourceReadError>,
        manifest: Result<Option<eidetic::ManifestId>, crate::research_storage::SourceReadError>,
    },
    Deposit {
        request: AuthorRequest,
        result: Result<eidetic::ManifestId, crate::research_storage::SourceReadError>,
    },
    Inspect {
        id: uuid::Uuid,
        scope: ScopeToken,
        binding: Binding,
        result: Result<PropertyRecordV1, crate::research_storage::SourceReadError>,
    },
    Replay {
        id: uuid::Uuid,
        scope: ScopeToken,
        result: Result<RecordResult, crate::research_storage::SourceReadError>,
    },
    ConfirmProperty {
        request: AuthorRequest,
        scope: ScopeToken,
        binding: Binding,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    ConfirmResult {
        request: QueryRequest,
        scope: ScopeToken,
        binding: Binding,
        value: RecordResult,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    MarkProperty {
        request: AuthorRequest,
        scope: ScopeToken,
        binding: Binding,
        assertion: serde_json::Value,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    MarkResult {
        request: QueryRequest,
        scope: ScopeToken,
        binding: Binding,
        assertion: serde_json::Value,
        value: RecordResult,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    PrepareWithdrawal {
        id: uuid::Uuid,
        scope: ScopeToken,
        binding: Binding,
        nonce: uuid::Uuid,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    ConfirmWithdrawal {
        id: uuid::Uuid,
        scope: ScopeToken,
        binding: Binding,
        nonce: uuid::Uuid,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    Recovery {
        id: uuid::Uuid,
        scope: ScopeToken,
        result: Result<
            Vec<crate::trail_memory::record_actor::RecoveryItem>,
            crate::research_storage::SourceReadError,
        >,
    },
    Reconcile {
        id: uuid::Uuid,
        scope: ScopeToken,
        item: crate::trail_memory::record_actor::RecoveryItem,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    RecoveryMark {
        id: uuid::Uuid,
        scope: ScopeToken,
        item: crate::trail_memory::record_actor::RecoveryItem,
        binding: Binding,
        assertion: serde_json::Value,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    RecoveryAck {
        id: uuid::Uuid,
        scope: ScopeToken,
        item: crate::trail_memory::record_actor::RecoveryItem,
        binding: Binding,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    Abandon {
        id: uuid::Uuid,
        scope: ScopeToken,
        nonce: uuid::Uuid,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    Discover {
        id: uuid::Uuid,
        scope: ScopeToken,
        bindings: Vec<Binding>,
        history: bool,
        result: Result<(), crate::research_storage::SourceReadError>,
    },
    OpenSource {
        id: uuid::Uuid,
        scope: ScopeToken,
        generation: uuid::Uuid,
        source: eidetic::models::recall::RecallSourceId,
        version: String,
        result: Result<Target, crate::research_storage::SourceReadError>,
    },
    OpenEvidence {
        id: uuid::Uuid,
        scope: ScopeToken,
        generation: Option<uuid::Uuid>,
        binding: Binding,
        record: PropertyRecordV1,
        result: Result<Target, crate::research_storage::SourceReadError>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityChoice {
    pub graph: crate::panes::GraphId,
    pub entity: uuid::Uuid,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedResultChoice {
    pub binding: Binding,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingChoice {
    pub nonce: uuid::Uuid,
    pub kind: String,
    pub status: String,
    pub orphan: bool,
}
