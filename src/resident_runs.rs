// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Durable, host-owned records for Servitor resident runs.
//!
//! The JSON DTO is intentionally narrow: it records tickets, correlations and
//! bounded operation identifiers, never action/effect payloads. `RunReducer`
//! is the only run state machine; replay below only reapplies its events.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use servitor::{
    Cap,
    Correlation,
    Effect,
    EffectKind,
    Lifecycle,
    Mode,
    ResultKind,
    RunEvent,
    RunHeader,
    RunId,
    RunLimits,
    RunPhase,
    RunReducer,
    RunTicket,
    Trigger,
    Usage,
};
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
impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            max_runs: 1024,
            max_events_per_run: 256,
            max_bytes: 512 * 1024
        }
    }
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
    pub member: Uuid
}

#[derive(serde::Serialize, serde::Deserialize)] #[serde(deny_unknown_fields)]
struct DiskState {
    version: u32,
    next_id: u64,
    runs: Vec<DiskRun>
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)] #[serde(deny_unknown_fields)]
struct DiskRun  {
    id: u64,
    session: Uuid,
    member: Uuid,
    header: DiskHeader,
    events: Vec<DiskEvent>,
    #[serde(default)] actions: Vec<DiskAction>
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)] #[serde(deny_unknown_fields)]
struct DiskAction {
    run_id: u64,
    step: u64,
    attempt: u64,
    generation: u64,
    name: String,
    outcome: Option<DiskActionOutcome>
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)] #[serde(rename_all="snake_case", deny_unknown_fields)]
enum DiskActionOutcome  {
    HandoffUntracked  {
        returned_effects: u32,
        local_refused: bool
    },
    StrictReconciliation  {
        returned_effects: u32,
        local_refused: bool
    }

}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)] #[serde(deny_unknown_fields)]
struct DiskHeader  {
    started_at_ms: u64,
    resident: String,
    subject: String,
    revision: String,
    generation: u64,
    lifecycle: String,
    trigger: DiskTrigger,
    required: Vec<DiskRequirement>,
    limits: DiskLimits
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)] #[serde(tag="kind", rename_all="snake_case", deny_unknown_fields)]
enum DiskTrigger {
    Manual,
    Journal {
        source: String,
        first: u64,
        last: u64
    },
    Clock {
        at_ms: u64
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)] #[serde(deny_unknown_fields)]
struct DiskRequirement {
    cap: String,
    mode: String
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)] #[serde(deny_unknown_fields)]
struct DiskLimits {
    decisions: u64,
    tool_calls: u64,
    tokens: u64,
    elapsed_ms: u64,
    consecutive_failures: u64
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)] #[serde(rename_all="snake_case")]
enum DiskResult {
    Progress,
    Completed,
    Refused,
    Failed,
    RetryableFailure
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)] #[serde(tag="kind", rename_all="snake_case", deny_unknown_fields)]
enum DiskEvent {
    Intent  {
        run_id: u64,
        step: u64,
        attempt: u64,
        generation: u64,
        operation: u64,
        consequential: bool,
        usage: DiskUsage,
        at_ms: u64
    },
    Result {
        run_id: u64,
        step: u64,
        attempt: u64,
        generation: u64,
        result: DiskResult,
        usage: DiskUsage,
        at_ms: u64
    },
    Cancel  {
        at_ms: u64
    },
    Paused  {
        at_ms: u64
    },
    Resumed  {
        at_ms: u64
    },
    Interrupted  {
        at_ms: u64
    },
    BudgetExhausted  {
        at_ms: u64
    },
    Reconciled {
        run_id: u64,
        step: u64,
        attempt: u64,
        generation: u64,
        result: DiskResult,
        usage: DiskUsage,
        at_ms: u64
    },
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)] #[serde(deny_unknown_fields)]
struct DiskUsage {
    decisions: u64,
    tool_calls: u64,
    tokens: u64,
    elapsed_ms: u64,
    consecutive_failures: u64
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join("denizens").join("resident-runs.json")
}
pub fn load_or_empty(dir: &Path) -> Result<ResidentRuns,
String> {
    load_or_empty_with_limits(dir,
    StorageLimits::default())
}
pub fn load_or_empty_with_limits(dir: &Path,
limits: StorageLimits) -> Result<ResidentRuns,
String> {
    match path(dir).try_exists() {
        Ok(false) => Ok(ResidentRuns::with_limits(limits)),
        Ok(true) => load_with_limits(dir,
        limits),
        Err(error) => Err(format!("cannot inspect resident run state: {error}")),
    }
}
pub fn load(dir: &Path) -> Result<ResidentRuns,
String> {
    load_with_limits(dir,
    StorageLimits::default())
}
pub fn load_with_limits(dir: &Path,
limits: StorageLimits) -> Result<ResidentRuns,
String> {
    let target = path(dir);
    let cap = limits.max_bytes.checked_add(1).ok_or_else(|| "resident run byte bound is invalid".to_string())?;
    let file = std::fs::File::open(&target).map_err(|error| format!("resident run state unreadable at {}: {error}",
    target.display()))?;
    let mut raw = Vec::new();
    file.take(cap).read_to_end(&mut raw).map_err(|error| format!("resident run state unreadable at {}: {error}",
    target.display()))?;
    if raw.len() as u64 > limits.max_bytes  {
        return Err("resident run state exceeds its configured storage bound".into());

    }
    let disk: DiskState = serde_json::from_slice(&raw).map_err(|error| format!("resident run state malformed at {}: {error}",
    target.display()))?;
    if disk.version != VERSION {
        return Err(format!("resident run state has unsupported version {}",
        disk.version));
    }
    let state = ResidentRuns {
        limits,
        next_id: disk.next_id,
        runs: disk.runs
    };
    state.validate()?;
    Ok(state)
}
pub fn save(dir: &Path,
state: &ResidentRuns) -> Result<(),
String> {
    state.validate()?;
    let target=path(dir);
    let raw=serde_json::to_vec_pretty(&DiskState {
        version: VERSION,
        next_id: state.next_id,
        runs: state.runs.clone()
    }).map_err(|e|format!("resident run state cannot encode: {e}"))?;
    if raw.len()as u64>state.limits.max_bytes {
        return Err("resident run state exceeds its configured storage bound".into())
    }
    let parent=target.parent().ok_or_else(||"resident run state has no parent".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e|format!("cannot create resident run directory: {e}"))?;
    let temp=target.with_extension("json.new");
    let mut file=std::fs::File::create(&temp).map_err(|e|format!("cannot create resident run state: {e}"))?;
    file.write_all(&raw).map_err(|e|format!("cannot write resident run state: {e}"))?;
    file.sync_all().map_err(|e|format!("cannot sync resident run state: {e}"))?;
    drop(file);
    std::fs::rename(temp,
    target).map_err(|e|format!("cannot commit resident run state: {e}"))
}

impl ResidentRuns {
    pub fn with_limits(limits: StorageLimits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }
    pub fn limits(&self) -> StorageLimits {
        self.limits
    }
    pub fn validate_session(&self,
    expected: Uuid) -> Result<(),
    String> {
        if self.runs.iter().all(|run|run.session==expected) {
            Ok(())
        }
        else {
            Err("resident run state belongs to another session".into())
        }
    }
    pub fn metadata(&self,
    id: RunId) -> Result<RunMetadata,
    String> {
        let run=self.runs.iter().find(|run|run.id==id.0).ok_or_else(||"unknown resident run".to_string())?;
        Ok(RunMetadata {
            session: run.session,
            member: run.member
        })
    }
    pub fn begin(&mut self,
    session: Uuid,
    member: Uuid,
    ticket: RunTicket,
    limits: RunLimits,
    at_ms: u64) -> Result<RunId,
    String> {
        if ticket.binding.id.0!=*member.as_bytes() {
            return Err("resident run ticket does not belong to its member".into())
        }
        if self.unresolved_for(session,
        member)?.is_some() {
            return Err("resident already has an unresolved run; reconcile or cancel it first".into())
        }
        if self.runs.len()>=self.limits.max_runs {
            return Err("resident run journal reached its configured bound; it will not evict receipts".into())
        }
        let id=self.next_id.checked_add(1).ok_or_else(||"resident run id exhausted".to_string())?;
        let header=RunHeader {
            id: RunId(id),
            ticket,
            started_at_ms: at_ms,
            limits
        };
        RunReducer::new(header.clone()).map_err(|e|format!("resident run header refused: {e:?}"))?;
        self.next_id=id;
        self.runs.push(DiskRun {
            id,
            session,
            member,
            header: DiskHeader::from(&header),
            events: vec![],
            actions: vec![]
        });
        Ok(RunId(id))
    }
    pub fn append(&mut self,
    id: RunId,
    event: RunEvent) -> Result<(),
    String> {
        let cap=self.limits.max_events_per_run;
        let run=self.runs.iter_mut().find(|r|r.id==id.0).ok_or_else(||"unknown resident run".to_string())?;
        if run.events.len()>=cap {
            return Err("resident run reached its configured event bound".into())
        }
        let mut reducer=run.reducer()?;
        reducer.apply(event).map_err(|e|format!("resident run event refused: {e:?}"))?;
        run.events.push(DiskEvent::from(event));
        Ok(())
    }
    pub fn reducer(&self,
    id: RunId) -> Result<RunReducer,
    String> {
        self.runs.iter().find(|r|r.id==id.0).ok_or_else(||"unknown resident run".to_string())?.reducer()
    }
    pub fn unresolved_for(&self,
    session: Uuid,
    member: Uuid) -> Result<Option<RunId>,
    String> {
        for run in &self.runs {
            if run.session==session&&run.member==member&&!matches!(run.reducer()?.phase(),
            RunPhase::Terminal(_)) {
                return Ok(Some(RunId(run.id)))
            }
        }
        Ok(None)
    }
    pub fn record_local_action_intent(&mut self,
    id: RunId,
    correlation: Correlation,
    name: &str) -> Result<(),
    String> {
        validate_action_name(name)?;
        let cap=self.limits.max_events_per_run;
        let run=self.runs.iter_mut().find(|run|run.id==id.0).ok_or_else(||"unknown resident run".to_string())?;
        if run.actions.len()>=cap {
            return Err("resident run action metadata reached its configured bound".into())
        }
        if !matches!(run.reducer()?.phase(),
        RunPhase::Awaiting {
            correlation: expected,
            ..
        }
        if expected==correlation)||correlation.run_id!=id {
            return Err("resident run action requires its awaiting intent".into())
        }
        if run.actions.iter().any(|action| {
            action.run_id == id.0
                && action.step == correlation.step
                && action.attempt == correlation.attempt
                && action.generation == correlation.generation
                && action.outcome.is_none()
        }) {
            return Err("resident run action intent is duplicate".into())
        }
        run.actions.push(DiskAction {
            run_id: id.0,
            step: correlation.step,
            attempt: correlation.attempt,
            generation: correlation.generation,
            name: name.into(),
            outcome: None
        });
        Ok(())
    }
    pub fn record_local_action_outcome(&mut self,
    id: RunId,
    correlation: Correlation,
    name: &str,
    returned_effects: u32,
    local_refused: bool,
    policy: ExternalEffectPolicy) -> Result<(),
    String> {
        validate_action_name(name)?;
        let run=self.runs.iter_mut().find(|run|run.id==id.0).ok_or_else(||"unknown resident run".to_string())?;
        if !matches!(run.reducer()?.phase(),
        RunPhase::Awaiting {
            correlation: expected,
            ..
        }
        if expected==correlation)||correlation.run_id!=id {
            return Err("resident run action requires its awaiting intent".into())
        }
        let action = run.actions.iter_mut().find(|action| {
            action.run_id == id.0
                && action.step == correlation.step
                && action.attempt == correlation.attempt
                && action.generation == correlation.generation
                && action.name == name
                && action.outcome.is_none()
        }).ok_or_else(|| "resident run action outcome has no matching intent".to_string())?;
        action.outcome=Some(match policy {
            ExternalEffectPolicy::HandoffUntracked => DiskActionOutcome::HandoffUntracked {
                returned_effects,
                local_refused
            },
            ExternalEffectPolicy::StrictReconciliation => DiskActionOutcome::StrictReconciliation {
                returned_effects,
                local_refused
            }
        });
        Ok(())
    }
    pub fn mark_interrupted(&mut self,
    at_ms: u64) -> Result<(),
    String> {
        let mut pending=Vec::new();
        for run in &self.runs {
            if matches!(run.reducer()?.phase(),
            RunPhase::Awaiting {
                ..
            }) {
                pending.push((RunId(run.id),
                at_ms.max(latest_at_ms(run))))
            }
        }
        for (id,
        clamped) in pending {
            self.append(id,
            RunEvent::Interrupted {
                at_ms: clamped
            })?
        }
        Ok(())
    }
    fn validate(&self) -> Result<(),
    String> {
        if self.runs.len()>self.limits.max_runs {
            return Err("resident run state exceeds its configured run bound".into())
        }
        let mut seen=std::collections::BTreeSet::new();
        for run in &self.runs {
            if run.id == 0
                || run.id > self.next_id
                || !seen.insert(run.id)
                || run.events.len() > self.limits.max_events_per_run
                || run.actions.len() > self.limits.max_events_per_run
            {
                return Err("resident run state has invalid bounds or duplicate ids".into())
            }
            if decode::<16>(&run.header.resident)?!=*run.member.as_bytes() {
                return Err("resident run member does not match its ticket".into())
            }
            if run.header.lifecycle!="Active" {
                return Err("stored resident run was not admitted from an active binding".into())
            }
            validate_actions(run)?;
            let _=run.reducer()?;
        }
        Ok(())
    }
}
impl DiskRun  {
    fn reducer(&self) -> Result<RunReducer,
    String> {
        let mut r=RunReducer::new(self.header.to_header(RunId(self.id))?).map_err(|e|format!("resident run header refused: {e:?}"))?;
        for event in self.events.iter().copied() {
            r.apply(event.to_event()?).map_err(|e|format!("resident run event is corrupt: {e:?}"))?;
        }
        Ok(r)
    }
}
impl DiskHeader  {
    fn to_header(&self,
    id: RunId) -> Result<RunHeader,
    String> {
        Ok(RunHeader {
            id,
            started_at_ms: self.started_at_ms,
            limits: self.limits.into(),
            ticket: RunTicket {
                binding: servitor::ResidentBinding {
                    id: servitor::ResidentId(decode::<16>(&self.resident)?),
                    subject: servitor::Subject::new(decode::<32>(&self.subject)?),
                    revision: servitor::BodyRevision(decode::<32>(&self.revision)?),
                    generation: self.generation,
                    lifecycle: parse_lifecycle(&self.lifecycle)?
                },
                trigger: self.trigger.to_trigger()?,
                required:self.required.iter().map(|x|Ok((Cap::parse(&x.cap).map_err(|_|"invalid stored capability")?,
                parse_mode(&x.mode)?))).collect::<Result<_,
                String>>()?
            }
        })
    }
}
impl From<&RunHeader> for DiskHeader  {
    fn from(h: &RunHeader) -> Self {
        Self {
            started_at_ms: h.started_at_ms,
            resident: hex(&h.ticket.binding.id.0),
            subject: h.ticket.binding.subject.to_hex(),
            revision: hex(&h.ticket.binding.revision.0),
            generation: h.ticket.binding.generation,
            lifecycle:format!("{:?}",
            h.ticket.binding.lifecycle),
            trigger: (&h.ticket.trigger).into(),
            required: h.ticket.required.iter().map(|(c,
            m)|DiskRequirement {
                cap: c.to_wire(),
                mode:format!("{m:?}")
            }).collect(),
            limits: h.limits.into()
        }
    }
}
impl DiskTrigger  {
    fn to_trigger(&self) -> Result<Trigger,
    String> {
        Ok(match self {
            Self::Manual => Trigger::Manual,
            Self::Journal {
                source,
                first,
                last
            }
            if !source.trim().is_empty() && first <= last => Trigger::Journal {
                source: source.clone(),
                first: *first,
                last: *last
            },
            Self::Clock {
                at_ms
            }
 => Trigger::Clock {
                at_ms: *at_ms
            },
            _=>return Err("invalid stored trigger".into())
        })
    }
}
impl From<&Trigger> for DiskTrigger  {
    fn from(t: &Trigger) -> Self {
        match t {
            Trigger::Manual => Self::Manual,
            Trigger::Journal {
                source,
                first,
                last
            }
 => Self::Journal {
                source: source.clone(),
                first: *first,
                last: *last
            },
            Trigger::Clock {
                at_ms
            }
 => Self::Clock {
                at_ms: *at_ms
            }
        }
    }
}
impl From<RunLimits> for DiskLimits  {
    fn from(v: RunLimits) -> Self {
        Self {
            decisions: v.decisions,
            tool_calls: v.tool_calls,
            tokens: v.tokens,
            elapsed_ms: v.elapsed_ms,
            consecutive_failures: v.consecutive_failures
        }
    }
}
impl From<DiskLimits> for RunLimits  {
    fn from(v: DiskLimits) -> Self {
        Self {
            decisions: v.decisions,
            tool_calls: v.tool_calls,
            tokens: v.tokens,
            elapsed_ms: v.elapsed_ms,
            consecutive_failures: v.consecutive_failures
        }
    }
}
impl From<Usage> for DiskUsage  {
    fn from(v: Usage) -> Self {
        Self {
            decisions: v.decisions,
            tool_calls: v.tool_calls,
            tokens: v.tokens,
            elapsed_ms: v.elapsed_ms,
            consecutive_failures: v.consecutive_failures
        }
    }
}
impl From<DiskUsage> for Usage  {
    fn from(v: DiskUsage) -> Self {
        Self {
            decisions: v.decisions,
            tool_calls: v.tool_calls,
            tokens: v.tokens,
            elapsed_ms: v.elapsed_ms,
            consecutive_failures: v.consecutive_failures
        }
    }
}
impl DiskEvent  {
    fn from(e: RunEvent) -> Self {
        use RunEvent::*;
        match e {
            IntentRecorded {
                correlation,
                effect,
                usage,
                at_ms
            }
 => Self::Intent {
                run_id: correlation.run_id.0,
                step: correlation.step,
                attempt: correlation.attempt,
                generation: correlation.generation,
                operation: effect.operation,
                consequential: effect.kind==EffectKind::NeedsReconciliation,
                usage: usage.into(),
                at_ms
            },
            ResultRecorded {
                correlation,
                result,
                usage,
                at_ms
            }
 => Self::Result {
                run_id: correlation.run_id.0,
                step: correlation.step,
                attempt: correlation.attempt,
                generation: correlation.generation,
                result: result.into(),
                usage: usage.into(),
                at_ms
            },
            CancellationRequested {
                at_ms
            }
 => Self::Cancel {
                at_ms
            },
            Paused {
                at_ms
            }
 => Self::Paused {
                at_ms
            },
            Resumed {
                at_ms
            }
 => Self::Resumed {
                at_ms
            },
            Interrupted {
                at_ms
            }
 => Self::Interrupted {
                at_ms
            },
            BudgetExhausted {
                at_ms
            }
 => Self::BudgetExhausted {
                at_ms
            },
            ReconciliationResolved {
                correlation,
                result,
                usage,
                at_ms
            }
 => Self::Reconciled {
                run_id: correlation.run_id.0,
                step: correlation.step,
                attempt: correlation.attempt,
                generation: correlation.generation,
                result: result.into(),
                usage: usage.into(),
                at_ms
            }
        }
    }
    fn to_event(self) -> Result<RunEvent,
    String> {
        let c=|id,
        s,
        a,
        g|Correlation {
            run_id: RunId(id),
            step: s,
            attempt: a,
            generation: g
        };
        Ok(match self {
            Self::Intent {
                run_id,
                step,
                attempt,
                generation,
                operation,
                consequential,
                usage,
                at_ms
            }
 => RunEvent::IntentRecorded {
                correlation: c(run_id,
                step,
                attempt,
                generation),
                effect: Effect {
                    kind: if consequential {
                        EffectKind::NeedsReconciliation
                    }
                    else {
                        EffectKind::ReadOnly
                    },
                    operation
                },
                usage: usage.into(),
                at_ms
            },
            Self::Result {
                run_id,
                step,
                attempt,
                generation,
                result,
                usage,
                at_ms
            }
 => RunEvent::ResultRecorded {
                correlation: c(run_id,
                step,
                attempt,
                generation),
                result: result.into(),
                usage: usage.into(),
                at_ms
            },
            Self::Cancel {
                at_ms
            }
 => RunEvent::CancellationRequested {
                at_ms
            },
            Self::Paused {
                at_ms
            }
 => RunEvent::Paused {
                at_ms
            },
            Self::Resumed {
                at_ms
            }
 => RunEvent::Resumed {
                at_ms
            },
            Self::Interrupted {
                at_ms
            }
 => RunEvent::Interrupted {
                at_ms
            },
            Self::BudgetExhausted {
                at_ms
            }
 => RunEvent::BudgetExhausted {
                at_ms
            },
            Self::Reconciled {
                run_id,
                step,
                attempt,
                generation,
                result,
                usage,
                at_ms
            }
 => RunEvent::ReconciliationResolved {
                correlation: c(run_id,
                step,
                attempt,
                generation),
                result: result.into(),
                usage: usage.into(),
                at_ms
            }
        })
    }
}
impl From<ResultKind> for DiskResult  {
    fn from(v: ResultKind) -> Self {
        match v {
            ResultKind::Progress => Self::Progress,
            ResultKind::Completed => Self::Completed,
            ResultKind::Refused => Self::Refused,
            ResultKind::Failed => Self::Failed,
            ResultKind::RetryableFailure => Self::RetryableFailure
        }
    }
}
impl From<DiskResult> for ResultKind  {
    fn from(v: DiskResult) -> Self {
        match v {
            DiskResult::Progress => Self::Progress,
            DiskResult::Completed => Self::Completed,
            DiskResult::Refused => Self::Refused,
            DiskResult::Failed => Self::Failed,
            DiskResult::RetryableFailure => Self::RetryableFailure
        }
    }
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x|format!("{x:02x}")).collect()
}
fn decode<const N: usize>(s: &str) -> Result<[u8;
N],
String> {
    if !s.is_ascii()||s.len()!=N*2 {
        return Err("invalid stored run identity".into())
    }
    let mut out=[0;
    N];
    for (i,
    b) in out.iter_mut().enumerate() {
        *b=u8::from_str_radix(&s[i*2..i*2+2],
        16).map_err(|_|"invalid stored run identity")?
    }
    Ok(out)
}
fn parse_mode(s: &str) -> Result<Mode,
String> {
    match s {
        "Read"=>Ok(Mode::Read),
        "Write"=>Ok(Mode::Write),
        "Delegate"=>Ok(Mode::Delegate),
        _=>Err("invalid stored run mode".into())
    }
}
fn parse_lifecycle(s: &str) -> Result<Lifecycle,
String> {
    match s {
        "Active"=>Ok(Lifecycle::Active),
        "Paused"=>Ok(Lifecycle::Paused),
        "Revoked"=>Ok(Lifecycle::Revoked),
        _=>Err("invalid stored lifecycle".into())
    }
}
fn latest_at_ms(run: &DiskRun) -> u64 {
    run.events.iter().fold(run.header.started_at_ms,
    |latest,
    event|latest.max(match event {
        DiskEvent::Intent {
            at_ms,
            ..
        }
        |DiskEvent::Result {
            at_ms,
            ..
        }
        |DiskEvent::Cancel {
            at_ms
        }
        |DiskEvent::Paused {
            at_ms
        }
        |DiskEvent::Resumed {
            at_ms
        }
        |DiskEvent::Interrupted {
            at_ms
        }
        |DiskEvent::BudgetExhausted {
            at_ms
        }
        |DiskEvent::Reconciled {
            at_ms,
            ..
        }
 => *at_ms
    }))
}
fn validate_action_name(name: &str) -> Result<(),
String> {
    if name.is_empty()||name.len()>160 {
        Err("resident run action name is invalid or exceeds its bound".into())
    }
    else {
        Ok(())
    }
}
fn validate_actions(run: &DiskRun) -> Result<(),
String> {
    let mut seen=std::collections::BTreeSet::new();
    for action in &run.actions {
        validate_action_name(&action.name)?;
        if action.run_id!=run.id||action.generation!=run.header.generation {
            return Err("resident run action metadata has invalid correlation".into())
        }
        if !seen.insert((action.run_id,
        action.step,
        action.attempt,
        action.generation,
        action.name.clone())) {
            return Err("resident run action metadata is duplicate".into())
        }
        if !run.events.iter().any(|event|matches!(event,
        DiskEvent::Intent {
            run_id,
            step,
            attempt,
            generation,
            ..
        }
        if *run_id==action.run_id&&*step==action.step&&*attempt==action.attempt&&*generation==action.generation)) {
            return Err("resident run action metadata has no persisted intent".into())
        }
    }
    Ok(())
}
