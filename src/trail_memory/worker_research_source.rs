// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    context: crate::research::Context,
    target: crate::saved_sources::Target,
    budget: Option<crate::research_storage::SharedReadBudget>,
    reply: std::sync::mpsc::SyncSender<
        Result<crate::saved_sources::Content, crate::research_storage::SourceReadError>,
    >,
    state: &mut Option<(FjallStore, BrowsingMemory)>,
    sealed_store: &mut Option<FjallStore>,
    custody: &Option<SourceCustody>,
    current_dir: &Path,
    current_capture_generation: uuid::Uuid,
    memory_error: &Option<crate::research_storage::SourceReadError>,
    sealed_error: &Option<crate::research_storage::SourceReadError>,
    limits: &Result<crate::research_storage::StorageLimits, String>,
) {
    let custody_epoch = custody.as_ref().and_then(SourceCustody::snapshot);
    let valid = context.generation == current_capture_generation
        && inference_owner(&current_dir).ok()
            == Some(format!("turnstone/{}", context.session.as_uuid()))
        && target.key.session == context.session;
    let mut result = if !valid {
        Err("The cited source belongs to another session opening.".into())
    } else if custody_epoch.is_none() {
        Err("Unlock the profile to read private saved sources.".into())
    } else if let Some(error) = &sealed_error {
        Err(error.clone())
    } else if let Some(admission) = context
        .admissions
        .iter()
        .find(|a| a.key == target.key && a.error.is_none())
    {
        match state.as_mut() {
            Some((store, _)) => {
                let budget = budget.or_else(|| limits.as_ref().ok().map(|limits| limits.budget()));
                match budget {
                    None => Err(limits
                        .as_ref()
                        .err()
                        .cloned()
                        .unwrap_or_else(|| "Storage limits unavailable".into())
                        .into()),
                    Some(budget) => {
                        let mut legacy = crate::research_storage::BudgetStore {
                            store,
                            budget: budget.clone(),
                        };
                        let mut sealed = sealed_store
                            .as_mut()
                            .map(|store| crate::research_storage::BudgetStore { store, budget });
                        saved_sources::resolve_with_sealed(
                            &mut legacy,
                            sealed.as_mut().map(|s| s as &mut dyn eidetic::Store),
                            custody
                                .as_ref()
                                .map(|v| v.sealer.as_ref() as &dyn eidetic::PayloadSealer),
                            admission,
                            &target,
                        )
                    },
                }
            },
            None => Err(memory_error
                .clone()
                .unwrap_or_else(|| "The session evidence store is unavailable.".into())),
        }
    } else {
        Err("The cited saved source was withdrawn from this session.".into())
    };
    if custody_epoch.is_some()
        && !custody
            .as_ref()
            .is_some_and(|binding| binding.remains_current(custody_epoch))
    {
        result = Err("Profile custody changed while reading a private cited source.".into());
    }
    let _ = reply.send(result);
}
