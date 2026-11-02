// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Apply the behavior's declared deadband at actuation. Evaluation has
    /// already happened, but none of its Actions have reached application
    /// truth yet. The baseline moves only when this subject actually adds a
    /// graph-journal entry.
    pub(super) fn lower_behavior_actions(
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
                if matches!(
                    run.phase(),
                    servitor::RunPhase::Terminal(servitor::TerminalOutcome::BudgetExhausted)
                ) {
                    self.events.push(AppEvent::DenizenRefused(format!(
                        "{label}: resident evaluation budget exhausted"
                    )));
                    return vec![Effect::Redraw];
                }
                let used = run.usage().decisions;
                if used
                    .checked_add(decisions)
                    .and_then(|total| total.checked_add(reserved_external_decision))
                    .is_none_or(|total| total > run.header().limits.decisions)
                    || run.usage().elapsed_ms > run.header().limits.elapsed_ms
                {
                    if let Err(error) = self.persist_resident_run_event(
                        run_id,
                        servitor::RunEvent::BudgetExhausted {
                            at_ms: self.resident_run_timestamp(run_id),
                        },
                    ) {
                        self.resident_run_error =
                            Some(format!("cannot persist exhausted resident budget: {error}"));
                    }
                    self.events.push(AppEvent::DenizenRefused(format!(
                        "{label}: resident action budget exhausted"
                    )));
                    return vec![Effect::Redraw];
                }
                match run.pending_correlation() {
                    Some(correlation) => correlation,
                    None => {
                        self.events.push(AppEvent::DenizenRefused(format!(
                            "{label}: run is not ready for lowering"
                        )));
                        return vec![Effect::Redraw];
                    },
                }
            },
            Err(error) => {
                self.events.push(AppEvent::DenizenRefused(format!(
                    "{label}: cannot inspect run before lowering: {error}"
                )));
                return vec![Effect::Redraw];
            },
        };
        if let Err(error) = self.persist_local_action_intent(
            captured_session,
            run_id,
            correlation,
            "application actions",
            decisions,
        ) {
            self.resident_run_error = Some(format!("cannot persist local action intent: {error}"));
            self.events.push(AppEvent::DenizenRefused(format!(
                "{label}: cannot record local action intent"
            )));
            return vec![Effect::Redraw];
        }
        let admission = if self.deadbands.get(subject).is_some() {
            let Some(at_ms) = self.now_ms else {
                let reason = format!("{label}: deadband requires a host-supplied clock");
                tracing::warn!(%reason, "participant actuation refused");
                self.events.push(AppEvent::DenizenRefused(reason));
                if let Err(error) = self.persist_local_action_outcome(
                    captured_session,
                    run_id,
                    correlation,
                    "application actions",
                    0,
                    servitor::ResultKind::Refused,
                    servitor::Usage::default(),
                ) {
                    self.resident_run_error =
                        Some(format!("cannot persist local action refusal: {error}"));
                }
                return vec![Effect::Redraw];
            };
            let Some(output) = output else {
                let reason = format!("{label}: declared deadband but did not call mere.output");
                tracing::warn!(%reason, "participant actuation refused");
                self.events.push(AppEvent::DenizenRefused(reason));
                if let Err(error) = self.persist_local_action_outcome(
                    captured_session,
                    run_id,
                    correlation,
                    "application actions",
                    0,
                    servitor::ResultKind::Refused,
                    servitor::Usage::default(),
                ) {
                    self.resident_run_error =
                        Some(format!("cannot persist local action refusal: {error}"));
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
                    if let Err(error) = self.persist_local_action_outcome(
                        captured_session,
                        run_id,
                        correlation,
                        "application actions",
                        0,
                        servitor::ResultKind::Refused,
                        servitor::Usage::default(),
                    ) {
                        self.resident_run_error =
                            Some(format!("cannot persist local action refusal: {error}"));
                    }
                    return vec![Effect::Redraw];
                },
            }
        } else {
            None
        };

        let before = match self.journal.lock() {
            Ok(journal) => journal.high_water(),
            Err(poisoned) => poisoned.into_inner().high_water(),
        };
        let lowering_origin = self.behavior_execution_origin();
        let lowering_started = Instant::now();
        let (effects, refused) = self.lower_denizen_actions(run_id, subject, label, actions);
        if self.session_id != captured_session
            || self.behavior_execution_origin() != lowering_origin
        {
            self.events.push(AppEvent::DenizenRefused(
                "session changed while resident actions were lowering; outcome remains unresolved"
                    .into(),
            ));
            return effects;
        }
        let elapsed_ms = u64::try_from(lowering_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let external_count = effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::SaveSession | Effect::Redraw))
            .count();
        let external_count = u32::try_from(external_count).unwrap_or(u32::MAX);
        let outcome = if refused {
            servitor::ResultKind::Refused
        } else {
            servitor::ResultKind::Completed
        };
        if let Err(error) = self.persist_local_action_outcome(
            captured_session,
            run_id,
            correlation,
            "application actions",
            external_count,
            outcome,
            servitor::Usage {
                elapsed_ms,
                ..servitor::Usage::default()
            },
        ) {
            self.resident_run_error = Some(format!("cannot persist local action outcome: {error}"));
            self.events.push(AppEvent::DenizenRefused(
                "resident action outcome became uncertain".into(),
            ));
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

    pub(super) fn persist_local_action_intent(
        &mut self,
        captured_session: crate::panes::SessionId,
        run_id: servitor::RunId,
        correlation: servitor::Correlation,
        label: &str,
        decisions: u64,
    ) -> Result<(), String> {
        let mut next = self.resident_runs.clone();
        if self.session_id != captured_session
            || next.metadata(run_id)?.session != *captured_session.as_uuid()
        {
            return Err("resident run session changed before local intent".into());
        }
        next.append(
            run_id,
            servitor::RunEvent::IntentRecorded {
                correlation,
                effect: servitor::Effect {
                    kind: servitor::EffectKind::NeedsReconciliation,
                    operation: 2,
                },
                usage: servitor::Usage {
                    decisions,
                    ..servitor::Usage::default()
                },
                at_ms: self.resident_run_timestamp(run_id),
            },
        )?;
        next.record_local_action_intent(run_id, correlation, label)?;
        crate::resident_runs::save(&self.session_dir(), &next)?;
        self.resident_runs = next;
        Ok(())
    }

    pub(super) fn persist_local_action_outcome(
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
        if self.session_id != captured_session
            || next.metadata(run_id)?.session != *captured_session.as_uuid()
        {
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
            next.append(
                run_id,
                servitor::RunEvent::ResultRecorded {
                    correlation,
                    result: outcome,
                    usage,
                    at_ms,
                },
            )?;
        } else {
            next.append(
                run_id,
                servitor::RunEvent::ResultRecorded {
                    correlation,
                    result: servitor::ResultKind::Progress,
                    usage,
                    at_ms,
                },
            )?;
            let external_correlation = next
                .reducer(run_id)?
                .pending_correlation()
                .ok_or_else(|| "resident run did not advance to external handoff".to_string())?;
            next.append(
                run_id,
                servitor::RunEvent::IntentRecorded {
                    correlation: external_correlation,
                    effect: servitor::Effect {
                        kind: servitor::EffectKind::NeedsReconciliation,
                        operation: 3,
                    },
                    usage: servitor::Usage {
                        decisions: 1,
                        ..servitor::Usage::default()
                    },
                    at_ms,
                },
            )?;
            if matches!(
                policy,
                crate::resident_runs::ExternalEffectPolicy::HandoffUntracked
            ) {
                next.append(
                    run_id,
                    servitor::RunEvent::ResultRecorded {
                        correlation: external_correlation,
                        result: outcome,
                        usage: servitor::Usage::default(),
                        at_ms,
                    },
                )?;
            }
        }
        crate::resident_runs::save(&self.session_dir(), &next)?;
        self.resident_runs = next;
        Ok(())
    }

    // Persist logical time monotonically even when the system clock moves back.
    // Authority expiry continues to use the actual host clock independently.
    pub(super) fn resident_run_timestamp(&self, run_id: servitor::RunId) -> u64 {
        let observed = crate::denizen::now_ms();
        self.resident_runs
            .reducer(run_id)
            .map(|run| observed.max(run.last_at_ms()))
            .unwrap_or(observed)
    }

    pub(super) fn persist_resident_run_event(
        &mut self,
        run_id: servitor::RunId,
        event: servitor::RunEvent,
    ) -> Result<(), String> {
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
            servitor::RunEvent::CancellationRequested {
                at_ms: self.resident_run_timestamp(run_id),
            },
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
        if !matches!(
            result,
            servitor::ResultKind::Completed
                | servitor::ResultKind::Failed
                | servitor::ResultKind::Refused
        ) {
            return Err("reconciliation requires a terminal outcome".into());
        }
        let at_ms = self.resident_run_timestamp(run_id);
        let mut next = self.resident_runs.clone();
        let run = next.reducer(run_id)?;
        match run.phase() {
            servitor::RunPhase::Awaiting {
                correlation: expected,
                effect,
            } if expected == correlation
                && effect.kind == servitor::EffectKind::NeedsReconciliation =>
            {
                next.append(run_id, servitor::RunEvent::Interrupted { at_ms })?;
            },
            servitor::RunPhase::Reconciling {
                correlation: expected,
                ..
            } if expected == correlation => {},
            servitor::RunPhase::Awaiting { .. } | servitor::RunPhase::Reconciling { .. } => {
                return Err("stale or mismatched resident run correlation".into());
            },
            _ => return Err("resident run is not awaiting reconciliation".into()),
        }
        next.append(
            run_id,
            servitor::RunEvent::ReconciliationResolved {
                correlation,
                result,
                usage: servitor::Usage {
                    consecutive_failures: u64::from(matches!(result, servitor::ResultKind::Failed)),
                    ..servitor::Usage::default()
                },
                at_ms,
            },
        )?;
        crate::resident_runs::save(&self.session_dir(), &next)?;
        self.resident_runs = next;
        Ok(())
    }

    pub(super) fn validate_resident_run_scope(
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

    pub(in super::super) fn install_denizen(&mut self, path: String) -> Vec<Effect> {
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
            },
            Err(err) => {
                tracing::warn!(%err, %path, "participant install refused at staging");
                self.events.push(AppEvent::DenizenRefused(err));
                vec![Effect::Redraw]
            },
        }
    }

    pub(in super::super) fn uninstall_denizen(&mut self, member: Uuid) -> Vec<Effect> {
        // Revocation, the mirror of install: the user's delegations to
        // this participant are revoked (cascading to anything it delegated
        // onward), and it stops residing. The node and its world are
        // untouched — revoking authority destroys nothing.
        let Some(_) = self.denizens.residents.get(&member) else {
            return vec![Effect::Redraw];
        };
        let mut admissions = self.denizens.admissions.clone();
        let Some(record) = admissions.get_mut(member) else {
            self.events.push(AppEvent::DenizenRefused(
                "participant has no durable admission record; uninstall refused".into(),
            ));
            return vec![Effect::Redraw];
        };
        record.binding.lifecycle = servitor::resident::Lifecycle::Revoked;
        let Some(generation) = record.binding.generation.checked_add(1) else {
            self.events.push(AppEvent::DenizenRefused(
                "participant lifecycle generation exhausted; uninstall refused".into(),
            ));
            return vec![Effect::Redraw];
        };
        record.binding.generation = generation;
        if let Err(err) = crate::resident_admission::save(&self.session_dir(), &admissions) {
            self.events.push(AppEvent::DenizenRefused(format!(
                "cannot record participant revocation; uninstall refused: {err}"
            )));
            return vec![Effect::Redraw];
        }
        let resident = self
            .denizens
            .residents
            .remove(&member)
            .expect("resident remained present after admission save");
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
        if let Err(error) = self
            .graph_runtimes
            .edit_facets(|facets| pandect::remove_denizen_binding(facets, member))
        {
            self.events.push(AppEvent::DenizenRefused(format!(
                "cannot remove participant binding: {error}"
            )));
            return vec![Effect::Redraw];
        }
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

    pub(in super::super) fn confirm_install_denizen(&mut self) -> Vec<Effect> {
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
