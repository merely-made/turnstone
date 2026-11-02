// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

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

    pub fn validate_session(&self, expected: Uuid) -> Result<(), String> {
        if self.runs.iter().all(|run| run.session == expected) {
            Ok(())
        } else {
            Err("resident run state belongs to another session".into())
        }
    }

    pub fn metadata(&self, id: RunId) -> Result<RunMetadata, String> {
        let run = self
            .runs
            .iter()
            .find(|run| run.id == id.0)
            .ok_or_else(|| "unknown resident run".to_string())?;
        Ok(RunMetadata {
            session: run.session,
            member: run.member,
        })
    }

    pub fn begin(
        &mut self,
        session: Uuid,
        member: Uuid,
        ticket: RunTicket,
        limits: RunLimits,
        at_ms: u64,
    ) -> Result<RunId, String> {
        if ticket.binding.id.0 != *member.as_bytes() {
            return Err("resident run ticket does not belong to its member".into());
        }
        if self.unresolved_for(session, member)?.is_some() {
            return Err(
                "resident already has an unresolved run; reconcile or cancel it first".into(),
            );
        }
        if self.runs.len() >= self.limits.max_runs {
            return Err(
                "resident run journal reached its configured bound; it will not evict receipts"
                    .into(),
            );
        }
        let id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| "resident run id exhausted".to_string())?;
        let header = RunHeader {
            id: RunId(id),
            ticket,
            started_at_ms: at_ms,
            limits,
        };
        RunReducer::new(header.clone())
            .map_err(|e| format!("resident run header refused: {e:?}"))?;
        self.next_id = id;
        self.runs.push(DiskRun {
            id,
            session,
            member,
            header: DiskHeader::from(&header),
            events: vec![],
            actions: vec![],
        });
        Ok(RunId(id))
    }

    pub fn append(&mut self, id: RunId, event: RunEvent) -> Result<(), String> {
        let cap = self.limits.max_events_per_run;
        let run = self
            .runs
            .iter_mut()
            .find(|r| r.id == id.0)
            .ok_or_else(|| "unknown resident run".to_string())?;
        if run.events.len() >= cap {
            return Err("resident run reached its configured event bound".into());
        }
        let mut reducer = run.reducer()?;
        reducer
            .apply(event)
            .map_err(|e| format!("resident run event refused: {e:?}"))?;
        run.events.push(DiskEvent::from(event));
        Ok(())
    }

    pub fn reducer(&self, id: RunId) -> Result<RunReducer, String> {
        self.runs
            .iter()
            .find(|r| r.id == id.0)
            .ok_or_else(|| "unknown resident run".to_string())?
            .reducer()
    }

    pub fn unresolved_for(&self, session: Uuid, member: Uuid) -> Result<Option<RunId>, String> {
        for run in &self.runs {
            if run.session == session
                && run.member == member
                && !matches!(run.reducer()?.phase(), RunPhase::Terminal(_))
            {
                return Ok(Some(RunId(run.id)));
            }
        }
        Ok(None)
    }

    pub fn record_local_action_intent(
        &mut self,
        id: RunId,
        correlation: Correlation,
        name: &str,
    ) -> Result<(), String> {
        validate_action_name(name)?;
        let cap = self.limits.max_events_per_run;
        let run = self
            .runs
            .iter_mut()
            .find(|run| run.id == id.0)
            .ok_or_else(|| "unknown resident run".to_string())?;
        if run.actions.len() >= cap {
            return Err("resident run action metadata reached its configured bound".into());
        }
        if !matches!(run.reducer()?.phase(),
        RunPhase::Awaiting {
            correlation: expected,
            ..
        }
        if expected==correlation)
            || correlation.run_id != id
        {
            return Err("resident run action requires its awaiting intent".into());
        }
        if run.actions.iter().any(|action| {
            action.run_id == id.0
                && action.step == correlation.step
                && action.attempt == correlation.attempt
                && action.generation == correlation.generation
                && action.outcome.is_none()
        }) {
            return Err("resident run action intent is duplicate".into());
        }
        run.actions.push(DiskAction {
            run_id: id.0,
            step: correlation.step,
            attempt: correlation.attempt,
            generation: correlation.generation,
            name: name.into(),
            outcome: None,
        });
        Ok(())
    }

    pub fn record_local_action_outcome(
        &mut self,
        id: RunId,
        correlation: Correlation,
        name: &str,
        returned_effects: u32,
        local_refused: bool,
        policy: ExternalEffectPolicy,
    ) -> Result<(), String> {
        validate_action_name(name)?;
        let run = self
            .runs
            .iter_mut()
            .find(|run| run.id == id.0)
            .ok_or_else(|| "unknown resident run".to_string())?;
        if !matches!(run.reducer()?.phase(),
        RunPhase::Awaiting {
            correlation: expected,
            ..
        }
        if expected==correlation)
            || correlation.run_id != id
        {
            return Err("resident run action requires its awaiting intent".into());
        }
        let action = run
            .actions
            .iter_mut()
            .find(|action| {
                action.run_id == id.0
                    && action.step == correlation.step
                    && action.attempt == correlation.attempt
                    && action.generation == correlation.generation
                    && action.name == name
                    && action.outcome.is_none()
            })
            .ok_or_else(|| "resident run action outcome has no matching intent".to_string())?;
        action.outcome = Some(match policy {
            ExternalEffectPolicy::HandoffUntracked => DiskActionOutcome::HandoffUntracked {
                returned_effects,
                local_refused,
            },
            ExternalEffectPolicy::StrictReconciliation => DiskActionOutcome::StrictReconciliation {
                returned_effects,
                local_refused,
            },
        });
        Ok(())
    }

    pub fn mark_interrupted(&mut self, at_ms: u64) -> Result<(), String> {
        let mut pending = Vec::new();
        for run in &self.runs {
            if matches!(run.reducer()?.phase(), RunPhase::Awaiting { .. }) {
                pending.push((RunId(run.id), at_ms.max(latest_at_ms(run))))
            }
        }
        for (id, clamped) in pending {
            self.append(id, RunEvent::Interrupted { at_ms: clamped })?
        }
        Ok(())
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self.runs.len() > self.limits.max_runs {
            return Err("resident run state exceeds its configured run bound".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for run in &self.runs {
            if run.id == 0
                || run.id > self.next_id
                || !seen.insert(run.id)
                || run.events.len() > self.limits.max_events_per_run
                || run.actions.len() > self.limits.max_events_per_run
            {
                return Err("resident run state has invalid bounds or duplicate ids".into());
            }
            if decode::<16>(&run.header.resident)? != *run.member.as_bytes() {
                return Err("resident run member does not match its ticket".into());
            }
            if run.header.lifecycle != "Active" {
                return Err("stored resident run was not admitted from an active binding".into());
            }
            validate_actions(run)?;
            let _ = run.reducer()?;
        }
        Ok(())
    }
}
