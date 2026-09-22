// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Gate-ring arms: install, uninstall, run a participant.
//!
//! Host-only by ring, so no grant can ever cover them: a component confirming
//! its own review would be self-escalation.

use uuid::Uuid;
use std::time::Instant;

use crate::action::{Action, Effect};
use crate::observe::AppEvent;
use crate::surface::FocusTarget;
use crate::ui::OmnibarState;

use super::App;

impl App {
    /// Run a resident because a watch woke it, rather than because somebody
    /// asked. The same lane either way: a behavior is a participant whose run was
    /// triggered, and giving it a second path would give it a second set of
    /// rules.
    pub(crate) fn run_denizen_for_cascade(
        &mut self,
        member: Uuid,
        trigger: &crate::behaviors::TriggerContext,
        scoped_reads: &[(servitor::Cap, servitor::Mode)],
    ) -> Vec<Effect> {
        let first = trigger.woken_by.first().map(|entry| entry.seq).unwrap_or(0);
        let last = trigger.woken_by.last().map(|entry| entry.seq).unwrap_or(0);
        self.run_denizen_with(
            member,
            trigger,
            servitor::resident::Trigger::Journal {
                source: "behavior".into(),
                first,
                last,
            },
            scoped_reads,
        )
    }

    pub(crate) fn run_denizen_for_clock(&mut self, member: Uuid, at_ms: u64) -> Vec<Effect> {
        self.run_denizen_with(
            member,
            &crate::behaviors::TriggerContext::default(),
            servitor::resident::Trigger::Clock { at_ms },
            &[],
        )
    }

    pub(super) fn run_denizen(&mut self, member: Uuid) -> Vec<Effect> {
        // Invoked by hand: the context is empty rather than absent, so a body
        // asking what woke it always gets an answer.
        self.run_denizen_with(
            member,
            &crate::behaviors::TriggerContext::default(),
            servitor::resident::Trigger::Manual,
            &[],
        )
    }

    fn run_denizen_with(
        &mut self,
        member: Uuid,
        trigger: &crate::behaviors::TriggerContext,
        admission_trigger: servitor::resident::Trigger,
        scoped_reads: &[(servitor::Cap, servitor::Mode)],
    ) -> Vec<Effect> {
        let run_session = self.session_id;
        self.denizens.authority.set_now(crate::denizen::now_ms());
        let Some((subject, label, binding)) = self
            .denizens
            .residents
            .get(&member)
            .map(|r| (r.subject, r.label.clone(), r.binding.clone()))
        else {
            return vec![Effect::Redraw];
        };
        if self.denizens.residents.values().filter(|resident| resident.subject == subject).count() != 1 {
            self.events.push(AppEvent::DenizenRefused("participant subject routing is ambiguous".into()));
            return vec![Effect::Redraw];
        }
        let mut required = vec![
            (crate::denizen::read_cap(), servitor::Mode::Read),
            (crate::denizen::world_cap(), servitor::Mode::Write),
        ];
        required.extend_from_slice(scoped_reads);
        let ticket = match servitor::resident::admit(
            &binding, admission_trigger, &self.denizens.authority, &required,
        ) {
            Ok(ticket) => ticket,
            Err(err) => {
                if matches!(err, servitor::resident::AdmissionError::Paused) {
                    let mut next = self.denizens.admissions.clone();
                    if let Some(record) = next.get_mut(member) {
                        let Some(skipped) = record.skipped.checked_add(1) else {
                            self.events.push(AppEvent::DenizenRefused(format!("{label}: paused; skip counter exhausted")));
                            return vec![Effect::Redraw];
                        };
                        record.skipped = skipped;
                    }
                    if let Err(save_err) = crate::resident_admission::save(&self.session_dir(), &next) {
                        self.events.push(AppEvent::DenizenRefused(format!("{label}: paused (cannot record skip: {save_err})")));
                    } else {
                        self.denizens.admissions = next;
                        self.events.push(AppEvent::DenizenRefused(format!("{label}: paused; trigger skipped")));
                    }
                } else {
                    self.events.push(AppEvent::DenizenRefused(format!("{label}: run admission refused: {err:?}")));
                }
                return vec![Effect::Redraw];
            }
        };
        if let Some(error) = &self.resident_run_error {
            self.events.push(AppEvent::DenizenRefused(format!("{label}: resident run store is unavailable: {error}")));
            return vec![Effect::Redraw];
        }
        // Admission alone is not a runnable invocation. Mint and sync the
        // host record before evaluating a body, so a crash has a durable
        // unresolved run rather than an unrecorded second execution.
        let started_at_ms = crate::denizen::now_ms();
        let mut runs = self.resident_runs.clone();
        let run_id = match runs.begin(
            *self.session_id.as_uuid(),
            member,
            ticket.clone(),
            self.resident_run_limits,
            started_at_ms,
        ) {
            Ok(id) => id,
            Err(error) => {
                self.events.push(AppEvent::DenizenRefused(format!("{label}: run refused: {error}")));
                return vec![Effect::Redraw];
            }
        };
        if let Err(error) = crate::resident_runs::save(&self.session_dir(), &runs) {
            self.resident_run_error = Some(format!("cannot persist resident run intent: {error}"));
            self.events.push(AppEvent::DenizenRefused(format!("{label}: cannot record run intent")));
            return vec![Effect::Redraw];
        }
        self.resident_runs = runs;
        let Some(correlation) = self.resident_runs.reducer(run_id).ok().and_then(|run| run.pending_correlation()) else {
            self.events.push(AppEvent::DenizenRefused(format!("{label}: new run is not ready")));
            return vec![Effect::Redraw];
        };
        if self.resident_runs.reducer(run_id).is_ok_and(|run| run.header().limits.decisions == 0) {
            if let Err(error) = self.persist_resident_run_event(run_id, servitor::RunEvent::BudgetExhausted {
                at_ms: self.resident_run_timestamp(run_id),
            }) {
                self.resident_run_error = Some(format!("cannot persist exhausted evaluation budget: {error}"));
            }
            self.events.push(AppEvent::DenizenRefused(format!("{label}: resident evaluation budget exhausted")));
            return vec![Effect::Redraw];
        }
        if let Err(error) = self.persist_resident_run_event(run_id, servitor::RunEvent::IntentRecorded {
            correlation,
            effect: servitor::Effect { kind: servitor::EffectKind::ReadOnly, operation: 1 },
            usage: servitor::Usage { decisions: 1, ..servitor::Usage::default() },
            at_ms: self.resident_run_timestamp(run_id),
        }) {
            self.events.push(AppEvent::DenizenRefused(format!("{label}: cannot record evaluation intent: {error}")));
            return vec![Effect::Redraw];
        }
        let evaluation_started = Instant::now();
        let evaluated = self.evaluate_denizen_body(member, subject, &label, &binding, trigger);
        let elapsed_ms = u64::try_from(evaluation_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (actions, output) = match evaluated {
            Ok(value) => {
                self.denizens.authority.set_now(crate::denizen::now_ms());
                match self.denizens.residents.get(&member).map(|resident| &resident.binding) {
                    Some(current) if servitor::resident::revalidate(&ticket, current, &self.denizens.authority, &required).is_ok() => value,
                    Some(_) => {
                        let reason = "run invalidated before lowering".to_string();
                        if let Err(error) = self.persist_resident_run_event(run_id, servitor::RunEvent::ResultRecorded {
                            correlation, result: servitor::ResultKind::Refused,
                            usage: servitor::Usage { elapsed_ms, ..servitor::Usage::default() },
                            at_ms: self.resident_run_timestamp(run_id),
                        }) { self.resident_run_error = Some(format!("cannot persist evaluation refusal: {error}")); }
                        self.events.push(AppEvent::DenizenRefused(format!("{label}: {reason}")));
                        return vec![Effect::Redraw];
                    }
                    None => {
                        if let Err(error) = self.persist_resident_run_event(run_id, servitor::RunEvent::ResultRecorded {
                            correlation, result: servitor::ResultKind::Refused,
                            usage: servitor::Usage { elapsed_ms, ..servitor::Usage::default() },
                            at_ms: self.resident_run_timestamp(run_id),
                        }) { self.resident_run_error = Some(format!("cannot persist evaluation refusal: {error}")); }
                        self.events.push(AppEvent::DenizenRefused(format!("{label}: resident disappeared before lowering")));
                        return vec![Effect::Redraw];
                    }
                }
            }
            Err((result, reason)) => {
                let usage = servitor::Usage {
                    elapsed_ms,
                    consecutive_failures: u64::from(matches!(result, servitor::ResultKind::Failed)),
                    ..servitor::Usage::default()
                };
                if let Err(error) = self.persist_resident_run_event(run_id, servitor::RunEvent::ResultRecorded {
                    correlation, result, usage, at_ms: self.resident_run_timestamp(run_id),
                }) {
                    self.resident_run_error = Some(format!("cannot persist evaluation outcome: {error}"));
                }
                self.events.push(AppEvent::DenizenRefused(format!("{label}: {reason}")));
                return vec![Effect::Redraw];
            }
        };
        if let Err(error) = self.persist_resident_run_event(run_id, servitor::RunEvent::ResultRecorded {
            correlation,
            result: servitor::ResultKind::Progress,
            usage: servitor::Usage { elapsed_ms, ..servitor::Usage::default() },
            at_ms: self.resident_run_timestamp(run_id),
        }) {
            self.resident_run_error = Some(format!("cannot persist evaluation result: {error}"));
            self.events.push(AppEvent::DenizenRefused(format!("{label}: cannot record evaluation result")));
            return vec![Effect::Redraw];
        }
        self.lower_behavior_actions(run_session, run_id, subject, label, actions, output)
    }

    fn evaluate_denizen_body(
        &mut self,
        member: Uuid,
        subject: servitor::Subject,
        label: &str,
        binding: &servitor::resident::ResidentBinding,
        trigger: &crate::behaviors::TriggerContext,
    ) -> Result<(Vec<Action>, Option<i64>), (servitor::ResultKind, String)> {
        let facet = |id: &str| {
            self.graph_runtimes
                .facets()
                .get(&member, &chartulary::FacetId::new(id))
                .and_then(|v| v.as_str().map(str::to_string))
        };
        // Which lane runs this resident is a property of what it IS
        // (a script's source facet, or a component's file pointer),
        // never of what it may DO — that is the grant's business.
        let component_file = facet(crate::denizen::COMPONENT_FACET);
        let source = facet(crate::denizen::SCENARIO_SOURCE_FACET);
        if let Some(file) = component_file {
            let path = crate::denizen::component_path(&self.session_dir(), &file);
            let bytes = std::fs::read(&path).ok();
            if bytes.as_deref().map(crate::resident_admission::body_revision) != Some(binding.revision) {
                return Err((servitor::ResultKind::Refused, "component body changed or is unreadable".into()));
            }
            #[cfg(not(feature = "wasm"))]
            {
                return Err((servitor::ResultKind::Refused, "this build carries no component runtime".into()));
            }
            #[cfg(feature = "wasm")]
            {
                let run = match crate::component::run_bytes(
                    bytes.as_deref().expect("body revision already accepted bytes"),
                    &self.denizens.authority,
                    subject,
                    "run",
                    &trigger.to_json(),
                ) {
                    Ok(run) => run,
                    Err(err) => {
                        return Err((servitor::ResultKind::Failed, format!("component run failed: {err}")));
                    }
                };
                for line in &run.logs { tracing::info!(%label, "{line}"); }
                for refusal in &run.refusals { tracing::info!(%label, "component emission refused: {refusal}"); }
                return Ok((run.actions, None));
            }
        }
        let Some(source) = source else {
            return Err((servitor::ResultKind::Refused, "resident has no runnable body".into()));
        };
        if crate::resident_admission::body_revision(source.as_bytes()) != binding.revision {
            return Err((servitor::ResultKind::Refused, "script body changed; reinstall and review it".into()));
        }
        // Evaluate the body (read-only against app truth; mutation
        // only ever leaves as typed Actions). The runnable lane is the
        // piccolo feature; a runtime-free build refuses honestly.
        #[cfg(not(feature = "piccolo"))]
        {
            let _ = (&source, &subject, trigger);
            tracing::warn!(%label, "participant run refused: built without the piccolo feature");
            return Err((servitor::ResultKind::Refused, "this build carries no script runtime".into()));
        }
        #[cfg(feature = "piccolo")]
        {
            let run = match crate::script::run_behavior(
                self,
                &source,
                // B2: what this run may do derives from the participant's
                // grant (the participant node), never a blanket flag.
                crate::script::capabilities_from_grant(&self.denizens.authority, subject),
                crate::denizen::RUN_BUDGET,
                trigger,
            ) {
                Ok(run) => run,
                Err(err) => {
                    tracing::warn!(%err, %label, "participant run failed");
                    return Err((servitor::ResultKind::Failed, err));
                }
            };
            Ok((run.actions, run.output))
        }
    }

    /// Host policy surface for a user-controlled pause. A pause records its
    /// generation before the live runtime sees it; queued replay is deferred.
    pub fn set_denizen_paused(&mut self, member: Uuid, paused: bool) -> Result<(), String> {
        let mut next = self.denizens.admissions.clone();
        let record = next.get_mut(member).ok_or_else(|| "participant has no admission record".to_string())?;
        if record.binding.lifecycle == servitor::resident::Lifecycle::Revoked {
            return Err("a revoked participant requires an explicit re-install".into());
        }
        record.binding.lifecycle = if paused { servitor::resident::Lifecycle::Paused } else { servitor::resident::Lifecycle::Active };
        record.binding.generation = record.binding.generation.checked_add(1)
            .ok_or_else(|| "participant lifecycle generation exhausted".to_string())?;
        let binding = record.binding.clone();
        crate::resident_admission::save(&self.session_dir(), &next)?;
        self.denizens.admissions = next;
        if let Some(resident) = self.denizens.residents.get_mut(&member) { resident.binding = binding; }
        Ok(())
    }

    /// Apply the behavior's declared deadband at actuation. Evaluation has
    /// already happened, but none of its Actions have reached application
    /// truth yet. The baseline moves only when this subject actually adds a
    /// graph-journal entry.
    fn lower_behavior_actions(
        &mut self,
        captured_session: crate::panes::SessionId,
        run_id: servitor::RunId,
        subject: servitor::Subject,
        label: String,
        actions: Vec<Action>,
        output: Option<i64>,
    ) -> Vec<Effect> {
        let decisions = u64::try_from(actions.len()).unwrap_or(u64::MAX).max(1);
        let reserved_external_decision = u64::from(!actions.is_empty());
        let correlation = match self.resident_runs.reducer(run_id) {
            Ok(run) => {
                if matches!(run.phase(), servitor::RunPhase::Terminal(servitor::TerminalOutcome::BudgetExhausted)) {
                    self.events.push(AppEvent::DenizenRefused(format!("{label}: resident evaluation budget exhausted")));
                    return vec![Effect::Redraw];
                }
                let used = run.usage().decisions;
                if used.checked_add(decisions)
                    .and_then(|total| total.checked_add(reserved_external_decision))
                    .is_none_or(|total| total > run.header().limits.decisions)
                    || run.usage().elapsed_ms > run.header().limits.elapsed_ms
                {
                    if let Err(error) = self.persist_resident_run_event(run_id, servitor::RunEvent::BudgetExhausted { at_ms: self.resident_run_timestamp(run_id) }) {
                        self.resident_run_error = Some(format!("cannot persist exhausted resident budget: {error}"));
                    }
                    self.events.push(AppEvent::DenizenRefused(format!("{label}: resident action budget exhausted")));
                    return vec![Effect::Redraw];
                }
                match run.pending_correlation() {
                    Some(correlation) => correlation,
                    None => {
                        self.events.push(AppEvent::DenizenRefused(format!("{label}: run is not ready for lowering")));
                        return vec![Effect::Redraw];
                    }
                }
            }
            Err(error) => {
                self.events.push(AppEvent::DenizenRefused(format!("{label}: cannot inspect run before lowering: {error}")));
                return vec![Effect::Redraw];
            }
        };
        if let Err(error) = self.persist_local_action_intent(captured_session, run_id, correlation, "application actions", decisions) {
            self.resident_run_error = Some(format!("cannot persist local action intent: {error}"));
            self.events.push(AppEvent::DenizenRefused(format!("{label}: cannot record local action intent")));
            return vec![Effect::Redraw];
        }
        let admission = if self.deadbands.get(subject).is_some() {
            let Some(at_ms) = self.now_ms else {
                let reason = format!("{label}: deadband requires a host-supplied clock");
                tracing::warn!(%reason, "participant actuation refused");
                self.events.push(AppEvent::DenizenRefused(reason));
                if let Err(error) = self.persist_local_action_outcome(captured_session, run_id, correlation, "application actions", 0, servitor::ResultKind::Refused, servitor::Usage::default()) {
                    self.resident_run_error = Some(format!("cannot persist local action refusal: {error}"));
                }
                return vec![Effect::Redraw];
            };
            let Some(output) = output else {
                let reason = format!("{label}: declared deadband but did not call mere.output");
                tracing::warn!(%reason, "participant actuation refused");
                self.events.push(AppEvent::DenizenRefused(reason));
                if let Err(error) = self.persist_local_action_outcome(captured_session, run_id, correlation, "application actions", 0, servitor::ResultKind::Refused, servitor::Usage::default()) {
                    self.resident_run_error = Some(format!("cannot persist local action refusal: {error}"));
                }
                return vec![Effect::Redraw];
            };
            match self
                .deadbands
                .check(subject, servitor::Actuation::new(output, at_ms))
            {
                Ok(admission) => Some(admission),
                Err(refusal) => {
                    let reason = format!("{label}: {refusal}");
                    tracing::warn!(%reason, "participant actuation refused");
                    self.events.push(AppEvent::DenizenRefused(reason));
                    if let Err(error) = self.persist_local_action_outcome(captured_session, run_id, correlation, "application actions", 0, servitor::ResultKind::Refused, servitor::Usage::default()) {
                        self.resident_run_error = Some(format!("cannot persist local action refusal: {error}"));
                    }
                    return vec![Effect::Redraw];
                }
            }
        } else {
            None
        };

        let before = match self.journal.lock() {
            Ok(journal) => journal.entries().len() as u64,
            Err(poisoned) => poisoned.into_inner().entries().len() as u64,
        };
        let lowering_started = Instant::now();
        let (effects, refused) = self.lower_denizen_actions(run_id, subject, label, actions);
        if self.session_id != captured_session {
            self.events.push(AppEvent::DenizenRefused("session changed while resident actions were lowering; outcome remains unresolved".into()));
            return effects;
        }
        let elapsed_ms = u64::try_from(lowering_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let external_count = effects.iter().filter(|effect| !matches!(effect, Effect::SaveSession | Effect::Redraw)).count();
        let external_count = u32::try_from(external_count).unwrap_or(u32::MAX);
        let outcome = if refused { servitor::ResultKind::Refused } else { servitor::ResultKind::Completed };
        if let Err(error) = self.persist_local_action_outcome(
            captured_session, run_id, correlation, "application actions", external_count, outcome,
            servitor::Usage { elapsed_ms, ..servitor::Usage::default() },
        ) {
            self.resident_run_error = Some(format!("cannot persist local action outcome: {error}"));
            self.events.push(AppEvent::DenizenRefused("resident action outcome became uncertain".into()));
        }
        let wrote = crate::behaviors::entries_since(self, before)
            .iter()
            .any(|entry| entry.author == subject.to_hex());
        if wrote && let Some(admission) = admission {
            self.deadbands.record(admission);
            crate::denizen::save_watches(
                &self.session_dir(),
                &self.watches,
                &self.app_watches,
                &self.time_watches,
                &self.deadbands,
            );
        }
        effects
    }

    fn persist_local_action_intent(
        &mut self,
        captured_session: crate::panes::SessionId,
        run_id: servitor::RunId,
        correlation: servitor::Correlation,
        label: &str,
        decisions: u64,
    ) -> Result<(), String> {
        let mut next = self.resident_runs.clone();
        if self.session_id != captured_session || next.metadata(run_id)?.session != *captured_session.as_uuid() {
            return Err("resident run session changed before local intent".into());
        }
        next.append(run_id, servitor::RunEvent::IntentRecorded {
            correlation,
            effect: servitor::Effect { kind: servitor::EffectKind::NeedsReconciliation, operation: 2 },
            usage: servitor::Usage { decisions, ..servitor::Usage::default() },
            at_ms: self.resident_run_timestamp(run_id),
        })?;
        next.record_local_action_intent(run_id, correlation, label)?;
        crate::resident_runs::save(&self.session_dir(), &next)?;
        self.resident_runs = next;
        Ok(())
    }

    fn persist_local_action_outcome(
        &mut self,
        captured_session: crate::panes::SessionId,
        run_id: servitor::RunId,
        correlation: servitor::Correlation,
        batch: &str,
        external_count: u32,
        outcome: servitor::ResultKind,
        usage: servitor::Usage,
    ) -> Result<(), String> {
        let at_ms = self.resident_run_timestamp(run_id);
        let mut next = self.resident_runs.clone();
        if self.session_id != captured_session || next.metadata(run_id)?.session != *captured_session.as_uuid() {
            return Err("resident run session changed before local outcome".into());
        }
        let policy = self.resident_run_effect_policy;
        next.record_local_action_outcome(
            run_id,
            correlation,
            batch,
            external_count,
            matches!(outcome, servitor::ResultKind::Refused),
            policy,
        )?;
        if external_count == 0 {
            next.append(run_id, servitor::RunEvent::ResultRecorded {
                correlation, result: outcome, usage, at_ms,
            })?;
        } else {
            next.append(run_id, servitor::RunEvent::ResultRecorded {
                correlation, result: servitor::ResultKind::Progress, usage, at_ms,
            })?;
            let external_correlation = next.reducer(run_id)?.pending_correlation()
                .ok_or_else(|| "resident run did not advance to external handoff".to_string())?;
            next.append(run_id, servitor::RunEvent::IntentRecorded {
                correlation: external_correlation,
                effect: servitor::Effect { kind: servitor::EffectKind::NeedsReconciliation, operation: 3 },
                usage: servitor::Usage { decisions: 1, ..servitor::Usage::default() },
                at_ms,
            })?;
            if matches!(policy, crate::resident_runs::ExternalEffectPolicy::HandoffUntracked) {
                next.append(run_id, servitor::RunEvent::ResultRecorded {
                    correlation: external_correlation, result: outcome,
                    usage: servitor::Usage::default(), at_ms,
                })?;
            }
        }
        crate::resident_runs::save(&self.session_dir(), &next)?;
        self.resident_runs = next;
        Ok(())
    }

    // Persist logical time monotonically even when the system clock moves back.
    // Authority expiry continues to use the actual host clock independently.
    fn resident_run_timestamp(&self, run_id: servitor::RunId) -> u64 {
        let observed = crate::denizen::now_ms();
        self.resident_runs.reducer(run_id)
            .map(|run| observed.max(run.last_at_ms()))
            .unwrap_or(observed)
    }

    fn persist_resident_run_event(&mut self, run_id: servitor::RunId, event: servitor::RunEvent) -> Result<(), String> {
        let mut next = self.resident_runs.clone();
        if next.metadata(run_id)?.session != *self.session_id.as_uuid() {
            return Err("resident run session changed before event persistence".into());
        }
        next.append(run_id, event)?;
        crate::resident_runs::save(&self.session_dir(), &next)?;
        self.resident_runs = next;
        Ok(())
    }

    /// Explicitly cancel a run in this adopted session. Cancellation does not
    /// fabricate a receipt for a consequential operation already in flight.
    pub fn cancel_resident_run(
        &mut self,
        captured_session: crate::panes::SessionId,
        run_id: servitor::RunId,
    ) -> Result<(), String> {
        self.validate_resident_run_scope(captured_session, run_id)?;
        self.persist_resident_run_event(
            run_id,
            servitor::RunEvent::CancellationRequested { at_ms: self.resident_run_timestamp(run_id) },
        )
    }

    /// Resolve the one exact pending correlation after the host has inspected
    /// its operation. Replay never invokes this path.
    pub fn reconcile_resident_run(
        &mut self,
        captured_session: crate::panes::SessionId,
        run_id: servitor::RunId,
        correlation: servitor::Correlation,
        result: servitor::ResultKind,
    ) -> Result<(), String> {
        self.validate_resident_run_scope(captured_session, run_id)?;
        if !matches!(result, servitor::ResultKind::Completed | servitor::ResultKind::Failed | servitor::ResultKind::Refused) {
            return Err("reconciliation requires a terminal outcome".into());
        }
        let at_ms = self.resident_run_timestamp(run_id);
        let mut next = self.resident_runs.clone();
        let run = next.reducer(run_id)?;
        match run.phase() {
            servitor::RunPhase::Awaiting { correlation: expected, effect }
                if expected == correlation && effect.kind == servitor::EffectKind::NeedsReconciliation =>
            {
                next.append(run_id, servitor::RunEvent::Interrupted { at_ms })?;
            }
            servitor::RunPhase::Reconciling { correlation: expected, .. } if expected == correlation => {}
            servitor::RunPhase::Awaiting { .. } | servitor::RunPhase::Reconciling { .. } => {
                return Err("stale or mismatched resident run correlation".into());
            }
            _ => return Err("resident run is not awaiting reconciliation".into()),
        }
        next.append(run_id, servitor::RunEvent::ReconciliationResolved {
            correlation,
            result,
            usage: servitor::Usage {
                consecutive_failures: u64::from(matches!(result, servitor::ResultKind::Failed)),
                ..servitor::Usage::default()
            },
            at_ms,
        })?;
        crate::resident_runs::save(&self.session_dir(), &next)?;
        self.resident_runs = next;
        Ok(())
    }

    fn validate_resident_run_scope(
        &self,
        captured_session: crate::panes::SessionId,
        run_id: servitor::RunId,
    ) -> Result<(), String> {
        if captured_session != self.session_id {
            return Err("resident run belongs to a stale adopted session".into());
        }
        let metadata = self.resident_runs.metadata(run_id)?;
        if metadata.session != *captured_session.as_uuid() {
            return Err("resident run belongs to another session namespace".into());
        }
        Ok(())
    }

    pub(super) fn install_denizen(&mut self, path: String) -> Vec<Effect> {
        match crate::denizen::stage_install(std::path::Path::new(&path)) {
            Ok(pending) => {
                self.events
                    .push(AppEvent::DenizenStaged(pending.label.clone()));
                self.pending_install = Some(pending);
                // Surface the review: the palette opens on the actions
                // lane, whose top rows are the Confirm (carrying the
                // ASK) and Cancel.
                self.omnibar = OmnibarState {
                    open: true,
                    text: ">".to_string(),
                    ..OmnibarState::default()
                };
                let target = self.fallback_shell_context();
                self.shell.begin_omnibar(target);
                self.focus = FocusTarget::Chrome;
                self.recompute_omnibar_suggestions();
                vec![Effect::Redraw]
            }
            Err(err) => {
                tracing::warn!(%err, %path, "participant install refused at staging");
                self.events.push(AppEvent::DenizenRefused(err));
                vec![Effect::Redraw]
            }
        }
    }

    pub(super) fn uninstall_denizen(&mut self, member: Uuid) -> Vec<Effect> {
        // Revocation, the mirror of install: the user's delegations to
        // this participant are revoked (cascading to anything it delegated
        // onward), and it stops residing. The node and its world are
        // untouched — revoking authority destroys nothing.
        let Some(_) = self.denizens.residents.get(&member) else {
            return vec![Effect::Redraw];
        };
        let mut admissions = self.denizens.admissions.clone();
        let Some(record) = admissions.get_mut(member) else {
            self.events.push(AppEvent::DenizenRefused("participant has no durable admission record; uninstall refused".into()));
            return vec![Effect::Redraw];
        };
        record.binding.lifecycle = servitor::resident::Lifecycle::Revoked;
        let Some(generation) = record.binding.generation.checked_add(1) else {
            self.events.push(AppEvent::DenizenRefused("participant lifecycle generation exhausted; uninstall refused".into()));
            return vec![Effect::Redraw];
        };
        record.binding.generation = generation;
        if let Err(err) = crate::resident_admission::save(&self.session_dir(), &admissions) {
            self.events.push(AppEvent::DenizenRefused(format!("cannot record participant revocation; uninstall refused: {err}")));
            return vec![Effect::Redraw];
        }
        let resident = self.denizens.residents.remove(&member).expect("resident remained present after admission save");
        self.denizens.admissions = admissions;
        let revoked = self.denizens.authority.revoke_root_grants(resident.subject);
        // A watch outliving its body would wake nothing, forever. Residency,
        // authority, and standing subscriptions end together.
        self.watches.remove_subject(resident.subject);
        self.app_watches.remove_subject(resident.subject);
        self.time_watches.remove_subject(resident.subject);
        self.deadbands.remove_subject(resident.subject);
        crate::denizen::save_watches(
            &self.session_dir(),
            &self.watches,
            &self.app_watches,
            &self.time_watches,
            &self.deadbands,
        );
        pandect::remove_denizen_binding(self.graph_runtimes.facets_mut(), member);
        let hex = resident.subject.to_hex();
        // The certificates go with the residency: a later adopt must
        // not resurrect the authority we just revoked.
        let path = crate::denizen::certs_path(&self.session_dir(), &hex);
        if path.is_file()
            && let Err(err) = std::fs::remove_file(&path)
        {
            tracing::warn!(%err, path = ?path, "failed to remove revoked certificates");
        }
        tracing::info!(label = %resident.label, revoked, "participant uninstalled");
        self.events
            .push(AppEvent::DenizenUninstalled(resident.label.clone()));
        vec![Effect::SaveSession, Effect::Redraw]
    }

    pub(super) fn confirm_install_denizen(&mut self) -> Vec<Effect> {
        let Some(pending) = self.pending_install.take() else {
            return vec![Effect::Redraw];
        };
        let label = pending.label.clone();
        if let Err(err) = crate::denizen::install(self, pending) {
            tracing::warn!(%err, "participant install refused before durable admission");
            self.events.push(AppEvent::DenizenRefused(err));
            return vec![Effect::Redraw];
        }
        self.events.push(AppEvent::DenizenInstalled(label));
        self.omnibar = OmnibarState::default();
        self.focus = FocusTarget::Graph(self.default_graph_pane());
        vec![Effect::SaveSession, Effect::Redraw]
    }
}
