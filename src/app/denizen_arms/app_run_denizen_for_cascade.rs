// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

    pub(in super::super) fn run_denizen(&mut self, member: Uuid) -> Vec<Effect> {
        // Invoked by hand: the context is empty rather than absent, so a body
        // asking what woke it always gets an answer.
        self.run_denizen_with(
            member,
            &crate::behaviors::TriggerContext::default(),
            servitor::resident::Trigger::Manual,
            &[],
        )
    }

    pub(super) fn run_denizen_with(
        &mut self,
        member: Uuid,
        trigger: &crate::behaviors::TriggerContext,
        admission_trigger: servitor::resident::Trigger,
        scoped_reads: &[(servitor::Cap, servitor::Mode)],
    ) -> Vec<Effect> {
        let run_session = self.session_id;
        let Some(run_origin) = self.behavior_execution_origin() else {
            self.refuse_behavior("participant run has no accepted session/graph binding".into());
            return vec![Effect::Redraw];
        };
        self.denizens.authority.set_now(crate::denizen::now_ms());
        let Some((subject, label, binding)) = self
            .denizens
            .residents
            .get(&member)
            .map(|r| (r.subject, r.label.clone(), r.binding.clone()))
        else {
            return vec![Effect::Redraw];
        };
        if self
            .denizens
            .residents
            .values()
            .filter(|resident| resident.subject == subject)
            .count()
            != 1
        {
            self.events.push(AppEvent::DenizenRefused(
                "participant subject routing is ambiguous".into(),
            ));
            return vec![Effect::Redraw];
        }
        let mut required = vec![
            (crate::denizen::read_cap(), servitor::Mode::Read),
            (crate::denizen::world_cap(), servitor::Mode::Write),
        ];
        required.extend_from_slice(scoped_reads);
        let ticket = match servitor::resident::admit(
            &binding,
            admission_trigger,
            &self.denizens.authority,
            &required,
        ) {
            Ok(ticket) => ticket,
            Err(err) => {
                if matches!(err, servitor::resident::AdmissionError::Paused) {
                    let mut next = self.denizens.admissions.clone();
                    if let Some(record) = next.get_mut(member) {
                        let Some(skipped) = record.skipped.checked_add(1) else {
                            self.events.push(AppEvent::DenizenRefused(format!(
                                "{label}: paused; skip counter exhausted"
                            )));
                            return vec![Effect::Redraw];
                        };
                        record.skipped = skipped;
                    }
                    if let Err(save_err) =
                        crate::resident_admission::save(&self.session_dir(), &next)
                    {
                        self.events.push(AppEvent::DenizenRefused(format!(
                            "{label}: paused (cannot record skip: {save_err})"
                        )));
                    } else {
                        self.denizens.admissions = next;
                        self.events.push(AppEvent::DenizenRefused(format!(
                            "{label}: paused; trigger skipped"
                        )));
                    }
                } else {
                    self.events.push(AppEvent::DenizenRefused(format!(
                        "{label}: run admission refused: {err:?}"
                    )));
                }
                return vec![Effect::Redraw];
            },
        };
        if let Some(error) = &self.resident_run_error {
            self.events.push(AppEvent::DenizenRefused(format!(
                "{label}: resident run store is unavailable: {error}"
            )));
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
                self.events.push(AppEvent::DenizenRefused(format!(
                    "{label}: run refused: {error}"
                )));
                return vec![Effect::Redraw];
            },
        };
        if let Err(error) = crate::resident_runs::save(&self.session_dir(), &runs) {
            self.resident_run_error = Some(format!("cannot persist resident run intent: {error}"));
            self.events.push(AppEvent::DenizenRefused(format!(
                "{label}: cannot record run intent"
            )));
            return vec![Effect::Redraw];
        }
        self.resident_runs = runs;
        let Some(correlation) = self
            .resident_runs
            .reducer(run_id)
            .ok()
            .and_then(|run| run.pending_correlation())
        else {
            self.events.push(AppEvent::DenizenRefused(format!(
                "{label}: new run is not ready"
            )));
            return vec![Effect::Redraw];
        };
        if self
            .resident_runs
            .reducer(run_id)
            .is_ok_and(|run| run.header().limits.decisions == 0)
        {
            if let Err(error) = self.persist_resident_run_event(
                run_id,
                servitor::RunEvent::BudgetExhausted {
                    at_ms: self.resident_run_timestamp(run_id),
                },
            ) {
                self.resident_run_error = Some(format!(
                    "cannot persist exhausted evaluation budget: {error}"
                ));
            }
            self.events.push(AppEvent::DenizenRefused(format!(
                "{label}: resident evaluation budget exhausted"
            )));
            return vec![Effect::Redraw];
        }
        if let Err(error) = self.persist_resident_run_event(
            run_id,
            servitor::RunEvent::IntentRecorded {
                correlation,
                effect: servitor::Effect {
                    kind: servitor::EffectKind::ReadOnly,
                    operation: 1,
                },
                usage: servitor::Usage {
                    decisions: 1,
                    ..servitor::Usage::default()
                },
                at_ms: self.resident_run_timestamp(run_id),
            },
        ) {
            self.events.push(AppEvent::DenizenRefused(format!(
                "{label}: cannot record evaluation intent: {error}"
            )));
            return vec![Effect::Redraw];
        }
        let evaluation_started = Instant::now();
        let evaluated = self.evaluate_denizen_body(member, subject, &label, &binding, trigger);
        let elapsed_ms =
            u64::try_from(evaluation_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if self.behavior_execution_origin() != Some(run_origin) {
            self.refuse_behavior("session or graph runtime changed during participant evaluation; outcome remains unresolved".into());
            return vec![Effect::Redraw];
        }
        let (actions, output) = match evaluated {
            Ok(value) => {
                self.denizens.authority.set_now(crate::denizen::now_ms());
                match self
                    .denizens
                    .residents
                    .get(&member)
                    .map(|resident| &resident.binding)
                {
                    Some(current)
                        if servitor::resident::revalidate(
                            &ticket,
                            current,
                            &self.denizens.authority,
                            &required,
                        )
                        .is_ok() =>
                    {
                        value
                    },
                    Some(_) => {
                        let reason = "run invalidated before lowering".to_string();
                        if let Err(error) = self.persist_resident_run_event(
                            run_id,
                            servitor::RunEvent::ResultRecorded {
                                correlation,
                                result: servitor::ResultKind::Refused,
                                usage: servitor::Usage {
                                    elapsed_ms,
                                    ..servitor::Usage::default()
                                },
                                at_ms: self.resident_run_timestamp(run_id),
                            },
                        ) {
                            self.resident_run_error =
                                Some(format!("cannot persist evaluation refusal: {error}"));
                        }
                        self.events
                            .push(AppEvent::DenizenRefused(format!("{label}: {reason}")));
                        return vec![Effect::Redraw];
                    },
                    None => {
                        if let Err(error) = self.persist_resident_run_event(
                            run_id,
                            servitor::RunEvent::ResultRecorded {
                                correlation,
                                result: servitor::ResultKind::Refused,
                                usage: servitor::Usage {
                                    elapsed_ms,
                                    ..servitor::Usage::default()
                                },
                                at_ms: self.resident_run_timestamp(run_id),
                            },
                        ) {
                            self.resident_run_error =
                                Some(format!("cannot persist evaluation refusal: {error}"));
                        }
                        self.events.push(AppEvent::DenizenRefused(format!(
                            "{label}: resident disappeared before lowering"
                        )));
                        return vec![Effect::Redraw];
                    },
                }
            },
            Err((result, reason)) => {
                let usage = servitor::Usage {
                    elapsed_ms,
                    consecutive_failures: u64::from(matches!(result, servitor::ResultKind::Failed)),
                    ..servitor::Usage::default()
                };
                if let Err(error) = self.persist_resident_run_event(
                    run_id,
                    servitor::RunEvent::ResultRecorded {
                        correlation,
                        result,
                        usage,
                        at_ms: self.resident_run_timestamp(run_id),
                    },
                ) {
                    self.resident_run_error =
                        Some(format!("cannot persist evaluation outcome: {error}"));
                }
                self.events
                    .push(AppEvent::DenizenRefused(format!("{label}: {reason}")));
                return vec![Effect::Redraw];
            },
        };
        if let Err(error) = self.persist_resident_run_event(
            run_id,
            servitor::RunEvent::ResultRecorded {
                correlation,
                result: servitor::ResultKind::Progress,
                usage: servitor::Usage {
                    elapsed_ms,
                    ..servitor::Usage::default()
                },
                at_ms: self.resident_run_timestamp(run_id),
            },
        ) {
            self.resident_run_error = Some(format!("cannot persist evaluation result: {error}"));
            self.events.push(AppEvent::DenizenRefused(format!(
                "{label}: cannot record evaluation result"
            )));
            return vec![Effect::Redraw];
        }
        self.lower_behavior_actions(run_session, run_id, subject, label, actions, output)
    }

    pub(super) fn evaluate_denizen_body(
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
            if bytes
                .as_deref()
                .map(crate::resident_admission::body_revision)
                != Some(binding.revision)
            {
                return Err((
                    servitor::ResultKind::Refused,
                    "component body changed or is unreadable".into(),
                ));
            }
            #[cfg(not(feature = "wasm"))]
            {
                return Err((
                    servitor::ResultKind::Refused,
                    "this build carries no component runtime".into(),
                ));
            }
            #[cfg(feature = "wasm")]
            {
                let run = match crate::component::run_bytes(
                    bytes
                        .as_deref()
                        .expect("body revision already accepted bytes"),
                    &self.denizens.authority,
                    subject,
                    "run",
                    &trigger.to_json(),
                ) {
                    Ok(run) => run,
                    Err(err) => {
                        return Err((
                            servitor::ResultKind::Failed,
                            format!("component run failed: {err}"),
                        ));
                    },
                };
                for line in &run.logs {
                    tracing::info!(%label, "{line}");
                }
                for refusal in &run.refusals {
                    tracing::info!(%label, "component emission refused: {refusal}");
                }
                return Ok((run.actions, None));
            }
        }
        let Some(source) = source else {
            return Err((
                servitor::ResultKind::Refused,
                "resident has no runnable body".into(),
            ));
        };
        if crate::resident_admission::body_revision(source.as_bytes()) != binding.revision {
            return Err((
                servitor::ResultKind::Refused,
                "script body changed; reinstall and review it".into(),
            ));
        }
        // Evaluate the body (read-only against app truth; mutation
        // only ever leaves as typed Actions). The runnable lane is the
        // piccolo feature; a runtime-free build refuses honestly.
        #[cfg(not(feature = "piccolo"))]
        {
            let _ = (&source, &subject, trigger);
            tracing::warn!(%label, "participant run refused: built without the piccolo feature");
            return Err((
                servitor::ResultKind::Refused,
                "this build carries no script runtime".into(),
            ));
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
                },
            };
            Ok((run.actions, run.output))
        }
    }

    /// Host policy surface for a user-controlled pause. A pause records its
    /// generation before the live runtime sees it; queued replay is deferred.
    pub fn set_denizen_paused(&mut self, member: Uuid, paused: bool) -> Result<(), String> {
        let mut next = self.denizens.admissions.clone();
        let record = next
            .get_mut(member)
            .ok_or_else(|| "participant has no admission record".to_string())?;
        if record.binding.lifecycle == servitor::resident::Lifecycle::Revoked {
            return Err("a revoked participant requires an explicit re-install".into());
        }
        record.binding.lifecycle = if paused {
            servitor::resident::Lifecycle::Paused
        } else {
            servitor::resident::Lifecycle::Active
        };
        record.binding.generation = record
            .binding
            .generation
            .checked_add(1)
            .ok_or_else(|| "participant lifecycle generation exhausted".to_string())?;
        let binding = record.binding.clone();
        crate::resident_admission::save(&self.session_dir(), &next)?;
        self.denizens.admissions = next;
        if let Some(resident) = self.denizens.residents.get_mut(&member) {
            resident.binding = binding;
        }
        Ok(())
    }
}
