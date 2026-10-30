// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl DiskEvent {
    pub(super) fn from(e: RunEvent) -> Self {
        use RunEvent::*;
        match e {
            IntentRecorded {
                correlation,
                effect,
                usage,
                at_ms,
            } => Self::Intent {
                run_id: correlation.run_id.0,
                step: correlation.step,
                attempt: correlation.attempt,
                generation: correlation.generation,
                operation: effect.operation,
                consequential: effect.kind == EffectKind::NeedsReconciliation,
                usage: usage.into(),
                at_ms,
            },
            ResultRecorded {
                correlation,
                result,
                usage,
                at_ms,
            } => Self::Result {
                run_id: correlation.run_id.0,
                step: correlation.step,
                attempt: correlation.attempt,
                generation: correlation.generation,
                result: result.into(),
                usage: usage.into(),
                at_ms,
            },
            CancellationRequested { at_ms } => Self::Cancel { at_ms },
            Paused { at_ms } => Self::Paused { at_ms },
            Resumed { at_ms } => Self::Resumed { at_ms },
            Interrupted { at_ms } => Self::Interrupted { at_ms },
            BudgetExhausted { at_ms } => Self::BudgetExhausted { at_ms },
            ReconciliationResolved {
                correlation,
                result,
                usage,
                at_ms,
            } => Self::Reconciled {
                run_id: correlation.run_id.0,
                step: correlation.step,
                attempt: correlation.attempt,
                generation: correlation.generation,
                result: result.into(),
                usage: usage.into(),
                at_ms,
            },
        }
    }

    pub(super) fn to_event(self) -> Result<RunEvent, String> {
        let c = |id, s, a, g| Correlation {
            run_id: RunId(id),
            step: s,
            attempt: a,
            generation: g,
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
                at_ms,
            } => RunEvent::IntentRecorded {
                correlation: c(run_id, step, attempt, generation),
                effect: Effect {
                    kind: if consequential {
                        EffectKind::NeedsReconciliation
                    } else {
                        EffectKind::ReadOnly
                    },
                    operation,
                },
                usage: usage.into(),
                at_ms,
            },
            Self::Result {
                run_id,
                step,
                attempt,
                generation,
                result,
                usage,
                at_ms,
            } => RunEvent::ResultRecorded {
                correlation: c(run_id, step, attempt, generation),
                result: result.into(),
                usage: usage.into(),
                at_ms,
            },
            Self::Cancel { at_ms } => RunEvent::CancellationRequested { at_ms },
            Self::Paused { at_ms } => RunEvent::Paused { at_ms },
            Self::Resumed { at_ms } => RunEvent::Resumed { at_ms },
            Self::Interrupted { at_ms } => RunEvent::Interrupted { at_ms },
            Self::BudgetExhausted { at_ms } => RunEvent::BudgetExhausted { at_ms },
            Self::Reconciled {
                run_id,
                step,
                attempt,
                generation,
                result,
                usage,
                at_ms,
            } => RunEvent::ReconciliationResolved {
                correlation: c(run_id, step, attempt, generation),
                result: result.into(),
                usage: usage.into(),
                at_ms,
            },
        })
    }
}
