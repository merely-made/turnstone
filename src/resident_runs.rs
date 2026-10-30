// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Durable, host-owned records for Servitor resident runs.
//!
//! The JSON DTO is intentionally narrow: it records tickets, correlations and
//! bounded operation identifiers, never action/effect payloads. `RunReducer`
//! is the only run state machine; replay below only reapplies its events.

use servitor::{
    Cap, Correlation, Effect, EffectKind, Lifecycle, Mode, ResultKind, RunEvent, RunHeader, RunId,
    RunLimits, RunPhase, RunReducer, RunTicket, Trigger, Usage,
};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExternalEffectPolicy {
    #[default]
    HandoffUntracked,
    StrictReconciliation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageLimits {
    pub max_runs: usize,
    pub max_events_per_run: usize,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ResidentRuns {
    limits: StorageLimits,
    next_id: u64,
    runs: Vec<DiskRun>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunMetadata {
    pub session: Uuid,
    pub member: Uuid,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskState {
    version: u32,
    next_id: u64,
    runs: Vec<DiskRun>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskRun {
    id: u64,
    session: Uuid,
    member: Uuid,
    header: DiskHeader,
    events: Vec<DiskEvent>,
    #[serde(default)]
    actions: Vec<DiskAction>,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskAction {
    run_id: u64,
    step: u64,
    attempt: u64,
    generation: u64,
    name: String,
    outcome: Option<DiskActionOutcome>,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum DiskActionOutcome {
    HandoffUntracked {
        returned_effects: u32,
        local_refused: bool,
    },
    StrictReconciliation {
        returned_effects: u32,
        local_refused: bool,
    },
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskHeader {
    started_at_ms: u64,
    resident: String,
    subject: String,
    revision: String,
    generation: u64,
    lifecycle: String,
    trigger: DiskTrigger,
    required: Vec<DiskRequirement>,
    limits: DiskLimits,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DiskTrigger {
    Manual,
    Journal {
        source: String,
        first: u64,
        last: u64,
    },
    Clock {
        at_ms: u64,
    },
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskRequirement {
    cap: String,
    mode: String,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskLimits {
    decisions: u64,
    tool_calls: u64,
    tokens: u64,
    elapsed_ms: u64,
    consecutive_failures: u64,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum DiskResult {
    Progress,
    Completed,
    Refused,
    Failed,
    RetryableFailure,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DiskEvent {
    Intent {
        run_id: u64,
        step: u64,
        attempt: u64,
        generation: u64,
        operation: u64,
        consequential: bool,
        usage: DiskUsage,
        at_ms: u64,
    },
    Result {
        run_id: u64,
        step: u64,
        attempt: u64,
        generation: u64,
        result: DiskResult,
        usage: DiskUsage,
        at_ms: u64,
    },
    Cancel {
        at_ms: u64,
    },
    Paused {
        at_ms: u64,
    },
    Resumed {
        at_ms: u64,
    },
    Interrupted {
        at_ms: u64,
    },
    BudgetExhausted {
        at_ms: u64,
    },
    Reconciled {
        run_id: u64,
        step: u64,
        attempt: u64,
        generation: u64,
        result: DiskResult,
        usage: DiskUsage,
        at_ms: u64,
    },
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskUsage {
    decisions: u64,
    tool_calls: u64,
    tokens: u64,
    elapsed_ms: u64,
    consecutive_failures: u64,
}

#[path = "resident_runs/impl_default_for_storage_limits.rs"]
mod impl_default_for_storage_limits;
#[path = "resident_runs/path.rs"]
mod path;
pub use path::path;
#[path = "resident_runs/load_or_empty.rs"]
mod load_or_empty;
pub use load_or_empty::load_or_empty;
#[path = "resident_runs/load_or_empty_with_limits.rs"]
mod load_or_empty_with_limits;
pub use load_or_empty_with_limits::load_or_empty_with_limits;
#[path = "resident_runs/load.rs"]
mod load;
pub use load::load;
#[path = "resident_runs/load_with_limits.rs"]
mod load_with_limits;
pub use load_with_limits::load_with_limits;
#[path = "resident_runs/save.rs"]
mod save;
pub use save::save;
#[path = "resident_runs/disk_event_from.rs"]
mod disk_event_from;
#[path = "resident_runs/disk_header_to_header.rs"]
mod disk_header_to_header;
#[path = "resident_runs/disk_run_reducer.rs"]
mod disk_run_reducer;
#[path = "resident_runs/disk_trigger_to_trigger.rs"]
mod disk_trigger_to_trigger;
#[path = "resident_runs/hex.rs"]
mod hex;
#[path = "resident_runs/impl_from_disk_limits_for.rs"]
mod impl_from_disk_limits_for;
#[path = "resident_runs/impl_from_disk_result_for.rs"]
mod impl_from_disk_result_for;
#[path = "resident_runs/impl_from_disk_usage_for.rs"]
mod impl_from_disk_usage_for;
#[path = "resident_runs/impl_from_result_kind_for.rs"]
mod impl_from_result_kind_for;
#[path = "resident_runs/impl_from_run_header_for.rs"]
mod impl_from_run_header_for;
#[path = "resident_runs/impl_from_run_limits_for.rs"]
mod impl_from_run_limits_for;
#[path = "resident_runs/impl_from_trigger_for_disk.rs"]
mod impl_from_trigger_for_disk;
#[path = "resident_runs/impl_from_usage_for_disk.rs"]
mod impl_from_usage_for_disk;
#[path = "resident_runs/resident_runs_with_limits.rs"]
mod resident_runs_with_limits;
use hex::hex;
#[path = "resident_runs/decode.rs"]
mod decode;
use decode::decode;
#[path = "resident_runs/parse_mode.rs"]
mod parse_mode;
use parse_mode::parse_mode;
#[path = "resident_runs/parse_lifecycle.rs"]
mod parse_lifecycle;
use parse_lifecycle::parse_lifecycle;
#[path = "resident_runs/latest_at_ms.rs"]
mod latest_at_ms;
use latest_at_ms::latest_at_ms;
#[path = "resident_runs/validate_action_name.rs"]
mod validate_action_name;
use validate_action_name::validate_action_name;
#[path = "resident_runs/validate_actions.rs"]
mod validate_actions;
use validate_actions::validate_actions;
