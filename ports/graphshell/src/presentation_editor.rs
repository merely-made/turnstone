// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Scoped declarative rule authoring. Preview returns decisions, never host actions.
//! The workspace record contains rules/history only, not graph or selection state.
use std::sync::{Arc, Mutex};

use edit_history::History;
use muniment::{Backend, WriteOp};
pub use scenograph::presentation;
use scenograph::presentation::{
    Condition, Context, Effect, Issue, PRESENTATION_RULE_VERSION, PrimitiveKind, Resolution, Rule,
    RuleSet, Target,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

const RECORD_LIMIT: usize = 1024 * 1024;
const HISTORY_LIMIT: usize = 32;

/// Muniment's default transaction method requires Sync on native hosts; the
/// browser's JS handles stay on the main thread and need no Sync promise.
#[cfg(not(target_arch = "wasm32"))]
pub trait RuleStore: Backend + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<B: Backend + Sync> RuleStore for B {}
#[cfg(target_arch = "wasm32")]
pub trait RuleStore: Backend {}
#[cfg(target_arch = "wasm32")]
impl<B: Backend> RuleStore for B {}

pub fn slot(graph_id: Uuid) -> String {
    format!("graphshell/presentation-rules/v1/{graph_id}")
}
fn empty() -> RuleSet {
    RuleSet {
        version: PRESENTATION_RULE_VERSION,
        rules: Vec::new(),
    }
}
fn json(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("validated presentation data serializes")
}

/// Raw fields remain editable and undoable even while malformed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleDraft {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub priority: String,
    pub target: String,
    pub condition: String,
    pub effects: String,
}
impl From<&Rule> for RuleDraft {
    fn from(rule: &Rule) -> Self {
        Self {
            id: rule.id.clone(),
            label: rule.label.clone(),
            enabled: rule.enabled,
            priority: rule.priority.to_string(),
            target: json(&rule.target),
            condition: json(&rule.condition),
            effects: json(&rule.effects),
        }
    }
}

/// Refuse unknown JSON properties in this editor rather than silently dropping
/// authored input. The shared resolver's compatibility policy is unchanged.
fn unknown(input: &Value, known: &Value, path: &str, issues: &mut Vec<Issue>) {
    match (input, known) {
        (Value::Object(input), Value::Object(known)) => {
            for (key, value) in input {
                let path = format!("{path}.{key}");
                if let Some(known) = known.get(key) {
                    unknown(value, known, &path, issues);
                } else {
                    issues.push(Issue {
                        field: path,
                        message: "unknown property".into(),
                    });
                }
            }
        },
        (Value::Array(input), Value::Array(known)) => {
            for (index, (input, known)) in input.iter().zip(known).enumerate() {
                unknown(input, known, &format!("{path}[{index}]"), issues);
            }
        },
        _ => {},
    }
}
pub fn parse<T: DeserializeOwned + Serialize>(text: &str, path: &str) -> Result<T, Vec<Issue>> {
    let invalid = |message: String| {
        vec![Issue {
            field: path.into(),
            message,
        }]
    };
    if text.len() > RECORD_LIMIT {
        return Err(invalid("input exceeds 1 MiB".into()));
    }
    let input: Value = serde_json::from_str(text).map_err(|e| invalid(e.to_string()))?;
    let value: T = serde_json::from_value(input.clone()).map_err(|e| invalid(e.to_string()))?;
    let mut issues = Vec::new();
    unknown(
        &input,
        &serde_json::to_value(&value).map_err(|e| invalid(e.to_string()))?,
        path,
        &mut issues,
    );
    if issues.is_empty() {
        Ok(value)
    } else {
        Err(issues)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RuleSetDraft {
    pub rules: Vec<RuleDraft>,
}
impl From<&RuleSet> for RuleSetDraft {
    fn from(rules: &RuleSet) -> Self {
        Self {
            rules: rules.rules.iter().map(RuleDraft::from).collect(),
        }
    }
}
impl RuleSetDraft {
    pub fn validate(&self) -> Result<RuleSet, Vec<Issue>> {
        if self.rules.len() > 256 {
            return Err(vec![Issue {
                field: "rules".into(),
                message: "at most 256 rules are supported".into(),
            }]);
        }
        let mut issues = Vec::new();
        let mut rules = Vec::new();
        for (index, draft) in self.rules.iter().enumerate() {
            let path = format!("rules[{index}]");
            let priority = draft.priority.trim().parse::<i32>().map_err(|_| {
                vec![Issue {
                    field: format!("{path}.priority"),
                    message: "priority must be a signed 32-bit integer".into(),
                }]
            });
            let target = parse::<Target>(&draft.target, &format!("{path}.target"));
            let condition = parse::<Condition>(&draft.condition, &format!("{path}.condition"));
            let effects = parse::<Vec<Effect>>(&draft.effects, &format!("{path}.effects"));
            match (priority, target, condition, effects) {
                (Ok(priority), Ok(target), Ok(condition), Ok(effects)) => rules.push(Rule {
                    id: draft.id.clone(),
                    label: draft.label.clone(),
                    enabled: draft.enabled,
                    priority,
                    target,
                    condition,
                    effects,
                }),
                (priority, target, condition, effects) => {
                    for errors in [priority.err(), target.err(), condition.err(), effects.err()]
                        .into_iter()
                        .flatten()
                    {
                        issues.extend(errors);
                    }
                },
            }
        }
        if !issues.is_empty() {
            return Err(issues);
        }
        let rules = RuleSet {
            version: PRESENTATION_RULE_VERSION,
            rules,
        };
        rules.validate()?;
        Ok(rules)
    }
    pub fn add(&mut self, kind: PrimitiveKind) -> usize {
        let prefix = match kind {
            PrimitiveKind::Node => "node",
            PrimitiveKind::Relation => "link",
            PrimitiveKind::Field => "field",
        };
        let mut number = 1;
        let id = loop {
            let candidate = format!("{prefix}-{number}");
            if !self.rules.iter().any(|r| r.id == candidate) {
                break candidate;
            }
            number += 1;
        };
        let effect = match kind {
            PrimitiveKind::Node => Effect::Details { visible: true },
            PrimitiveKind::Relation => Effect::Provenance { visible: true },
            PrimitiveKind::Field => Effect::RegionGuides { visible: true },
        };
        self.rules.push(RuleDraft::from(&Rule {
            id,
            label: format!("New {prefix} rule"),
            enabled: true,
            priority: 0,
            target: Target {
                kind,
                ids: Default::default(),
            },
            condition: Condition::Always,
            effects: vec![effect],
        }));
        self.rules.len() - 1
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u16,
    graph_id: Uuid,
    generation: u64,
    rules: RuleSet,
    undo: Vec<RuleSet>,
    redo: Vec<RuleSet>,
}
impl Record {
    fn validate(&self, graph_id: Uuid) -> Result<(), String> {
        if self.version != 1
            || self.graph_id != graph_id
            || self.undo.len() > HISTORY_LIMIT
            || self.redo.len() > HISTORY_LIMIT
        {
            return Err(
                "invalid presentation workspace identity, version or history length".into(),
            );
        }
        for rules in std::iter::once(&self.rules)
            .chain(&self.undo)
            .chain(&self.redo)
        {
            rules
                .validate()
                .map_err(|e| format!("Invalid saved presentation rules: {e:?}"))?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveKind {
    Apply,
    Undo,
    Redo,
}

#[derive(Clone)]
pub struct PreparedSave {
    key: String,
    expected: Option<Vec<u8>>,
    bytes: Vec<u8>,
}
impl PreparedSave {
    /// Compare and replace inside the backend transaction; stale tabs cannot
    /// overwrite a newer workspace. Nontransactional backends refuse this save.
    pub async fn save<B: RuleStore>(&self, backend: B) -> Result<(), String> {
        let key = self.key.clone();
        let expected = self.expected.clone();
        let bytes = self.bytes.clone();
        let outcome = Arc::new(Mutex::new(None));
        let answer = outcome.clone();
        backend.transact(Box::new(move |reader| {
            let result = reader.get(&key).map_err(|e| e.to_string()).and_then(|held| {
                if held == expected { Ok(()) } else { Err("Saved rules changed elsewhere; draft retained. Reopen or reconcile before applying.".into()) }
            });
            let accepted = result.is_ok();
            *answer.lock().expect("transaction result") = Some(result);
            if accepted { vec![WriteOp::Put { key, value: bytes }] } else { Vec::new() }
        })).await.map_err(|e| e.to_string())?;
        outcome
            .lock()
            .expect("transaction result")
            .take()
            .ok_or("backend did not run the transaction")?
    }
}
struct Draft {
    value: RuleSetDraft,
    history: History<RuleSetDraft, String>,
}
pub struct PresentationSession {
    saved: Record,
    baseline: Option<Vec<u8>>,
    draft: Option<Draft>,
    pending: Option<(Record, PreparedSave)>,
}
impl PresentationSession {
    pub fn new(graph_id: Uuid) -> Self {
        Self {
            saved: Record {
                version: 1,
                graph_id,
                generation: 0,
                rules: empty(),
                undo: Vec::new(),
                redo: Vec::new(),
            },
            baseline: None,
            draft: None,
            pending: None,
        }
    }
    pub async fn load<B: Backend>(backend: B, graph_id: Uuid) -> Result<Self, String> {
        let baseline = backend
            .get(&slot(graph_id))
            .await
            .map_err(|e| e.to_string())?;
        let mut session = Self::new(graph_id);
        if let Some(bytes) = &baseline {
            if bytes.len() > RECORD_LIMIT {
                return Err("presentation workspace exceeds 1 MiB".into());
            }
            let text = std::str::from_utf8(bytes)
                .map_err(|e| format!("Invalid presentation workspace: {e}"))?;
            session.saved = parse(text, "workspace")
                .map_err(|e| format!("Invalid presentation workspace: {e:?}"))?;
            session.saved.validate(graph_id)?;
        }
        session.baseline = baseline;
        Ok(session)
    }
    pub fn committed(&self) -> &RuleSet {
        &self.saved.rules
    }
    pub fn draft(&self) -> Option<&RuleSetDraft> {
        self.draft.as_ref().map(|d| &d.value)
    }
    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn dirty(&self) -> bool {
        self.draft()
            .is_some_and(|d| d != &RuleSetDraft::from(&self.saved.rules))
    }
    pub fn generation(&self) -> u64 {
        self.saved.generation
    }
    pub fn can_undo(&self) -> bool {
        !self.busy()
            && self
                .draft
                .as_ref()
                .map_or(!self.saved.undo.is_empty(), |d| d.history.can_undo())
    }
    pub fn can_redo(&self) -> bool {
        !self.busy()
            && self
                .draft
                .as_ref()
                .map_or(!self.saved.redo.is_empty(), |d| d.history.can_redo())
    }
    fn idle(&self) -> Result<(), String> {
        if self.busy() {
            Err("Wait for the presentation save to finish".into())
        } else {
            Ok(())
        }
    }
    pub fn begin(&mut self) -> Result<(), String> {
        self.idle()?;
        if self.draft.is_none() {
            self.draft = Some(Draft {
                value: RuleSetDraft::from(&self.saved.rules),
                history: History::new().with_cap(HISTORY_LIMIT).with_window(400),
            });
        }
        Ok(())
    }
    pub fn edit(
        &mut self,
        key: Option<String>,
        now_ms: u64,
        change: impl FnOnce(&mut RuleSetDraft),
    ) -> Result<(), String> {
        self.idle()?;
        let draft = self.draft.as_mut().ok_or("Begin a rule draft first")?;
        let mut next = draft.value.clone();
        change(&mut next);
        // Bound malformed drafts as well as valid saved records.
        let size: usize = next
            .rules
            .iter()
            .map(|r| {
                r.id.len()
                    + r.label.len()
                    + r.priority.len()
                    + r.target.len()
                    + r.condition.len()
                    + r.effects.len()
            })
            .sum();
        if next.rules.len() > 256 || size > RECORD_LIMIT {
            return Err("rule draft exceeds its 256-rule / 1 MiB limit".into());
        }
        if next != draft.value {
            draft.history.record(draft.value.clone(), key, now_ms);
            draft.value = next;
        }
        Ok(())
    }
    pub fn undo_draft(&mut self, redo: bool) -> Result<(), String> {
        self.idle()?;
        let draft = self.draft.as_mut().ok_or("No rule draft is open")?;
        let next = if redo {
            draft.history.redo(draft.value.clone())
        } else {
            draft.history.undo(draft.value.clone())
        };
        if let Some(next) = next {
            draft.value = next;
        }
        Ok(())
    }
    pub fn discard(&mut self) -> Result<(), String> {
        self.idle()?;
        self.draft = None;
        Ok(())
    }
    pub fn rules(&self) -> Result<RuleSet, Vec<Issue>> {
        self.draft()
            .map_or_else(|| Ok(self.saved.rules.clone()), RuleSetDraft::validate)
    }
    /// Pure preview: prior matches are explicit input, never silently shared
    /// between occurrences, samples, committed rules and drafts.
    pub fn preview(&self, context: &Context) -> Result<Resolution, Vec<Issue>> {
        self.rules()?.resolve(context)
    }
    pub fn prepare_save(&mut self, kind: SaveKind) -> Result<PreparedSave, String> {
        self.idle()?;
        let mut next = self.saved.clone();
        match kind {
            SaveKind::Apply => {
                let rules = self
                    .draft()
                    .ok_or("No rule draft is open")?
                    .validate()
                    .map_err(|e| format!("Rule draft refused: {e:?}"))?;
                if rules == next.rules {
                    return Err("No rule changes to apply".into());
                }
                next.undo.push(next.rules.clone());
                if next.undo.len() > HISTORY_LIMIT {
                    next.undo.remove(0);
                }
                next.redo.clear();
                next.rules = rules;
            },
            SaveKind::Undo | SaveKind::Redo => {
                if self.editing() {
                    return Err("Discard or apply the draft before saved undo".into());
                }
                let (from, to) = if kind == SaveKind::Undo {
                    (&mut next.undo, &mut next.redo)
                } else {
                    (&mut next.redo, &mut next.undo)
                };
                let rules = from.pop().ok_or("No saved rule step is available")?;
                to.push(next.rules.clone());
                next.rules = rules;
            },
        }
        next.generation = next
            .generation
            .checked_add(1)
            .ok_or("presentation generation exhausted")?;
        next.validate(self.saved.graph_id)?;
        let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        if bytes.len() > RECORD_LIMIT {
            return Err("saved rule history exceeds 1 MiB; draft retained".into());
        }
        let proposal = PreparedSave {
            key: slot(next.graph_id),
            expected: self.baseline.clone(),
            bytes,
        };
        self.pending = Some((next, proposal.clone()));
        Ok(proposal)
    }
    pub fn finish_save(&mut self, result: Result<(), String>) -> Result<(), String> {
        let (next, proposal) = self
            .pending
            .take()
            .ok_or("No presentation save is pending")?;
        result?;
        self.saved = next;
        self.baseline = Some(proposal.bytes);
        self.draft = None;
        Ok(())
    }
}

#[cfg(test)]
#[path = "presentation_editor_tests.rs"]
mod tests;
