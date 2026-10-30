// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn validate_actions(run: &DiskRun) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for action in &run.actions {
        validate_action_name(&action.name)?;
        if action.run_id != run.id || action.generation != run.header.generation {
            return Err("resident run action metadata has invalid correlation".into());
        }
        if !seen.insert((
            action.run_id,
            action.step,
            action.attempt,
            action.generation,
            action.name.clone(),
        )) {
            return Err("resident run action metadata is duplicate".into());
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
