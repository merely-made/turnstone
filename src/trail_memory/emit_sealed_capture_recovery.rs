// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn emit_sealed_capture_recovery(
    store: &mut FjallStore,
    custody: &SourceCustody,
    directory: &Path,
    capture_generation: uuid::Uuid,
    out: &Emitter<Update>,
    limits: &crate::research_storage::StorageLimits,
) {
    let Some(custody_epoch) = custody.snapshot() else {
        return;
    };
    let mut updates = Vec::new();
    let mut bounded = crate::research_storage::BudgetStore {
        store,
        budget: limits.budget(),
    };
    match sealed_capture::receipts(&mut bounded, custody.sealer.as_ref()) {
        Ok(receipts) => {
            for receipt in receipts {
                let owner = directory
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str());
                if owner != Some(receipt.request.session.as_uuid().to_string().as_str()) {
                    updates.push(Update::SourceCaptureRecoveryFailed {
                        capture_generation,
                        error: "sealed capture owner differs from session".into(),
                    });
                    continue;
                }
                if receipt.saved {
                    if let Some(stored) = receipt.stored {
                        if let Err(error) = sealed_capture::verify_typed(
                            &mut bounded,
                            custody.sealer.as_ref(),
                            &receipt.request,
                            &stored,
                        ) {
                            let error = error.context(&format!(
                                "sealed capture request {} verification failed",
                                receipt.request.id
                            ));
                            if error.storage_error().is_some() {
                                updates.clear();
                                updates.push(Update::SourceCaptureRecoveryFailed {
                                    capture_generation,
                                    error: error.into(),
                                });
                                break;
                            }
                            updates.push(Update::SourceCaptureRecoveryFailed {
                                capture_generation,
                                error: error.into(),
                            });
                        } else {
                            updates.push(Update::SourceCaptureRevalidated {
                                capture_generation,
                                request: receipt.request,
                                stored,
                            });
                        }
                    } else {
                        updates.push(Update::SourceCaptureRecoveryFailed {
                            capture_generation,
                            error: "sealed capture receipt missing deposit".into(),
                        });
                    }
                } else {
                    updates.push(Update::SourceCaptureRecovered {
                        capture_generation,
                        request: receipt.request,
                        stored: receipt.stored,
                    });
                }
            }
        },
        Err(error) => updates.push(Update::SourceCaptureRecoveryFailed {
            capture_generation,
            error: error.into(),
        }),
    }
    for update in updates {
        if !custody.remains_current(Some(custody_epoch)) {
            break;
        }
        out.emit(update);
    }
}
