// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn emit_capture_recovery(
    state: &mut Option<(FjallStore, BrowsingMemory)>,
    custody: &SourceCustody,
    directory: &Path,
    capture_generation: uuid::Uuid,
    out: &Emitter<Update>,
    limits: &crate::research_storage::StorageLimits,
) {
    let Some(custody_epoch) = custody.snapshot() else {
        return;
    };
    let Some((store, _)) = state.as_mut() else {
        return;
    };
    let mut updates = Vec::new();
    let mut bounded = crate::research_storage::BudgetStore {
        store,
        budget: limits.budget(),
    };
    match source_capture_receipts::recover_receipts(&mut bounded) {
        Ok(receipts) => {
            for receipt in receipts {
                // Directory admission comes from the live actor, not saved JSON.
                let owner = directory
                    .parent()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str());
                if owner != Some(receipt.request.session.as_uuid().to_string().as_str()) {
                    updates.push(Update::SourceCaptureRecoveryFailed {
                        capture_generation,
                        error: "capture receipt owner does not match the admitted session store"
                            .into(),
                    });
                    continue;
                }
                if receipt.saved {
                    if let Err(error) =
                        source_capture_receipts::verify_receipt_typed(&mut bounded, &receipt)
                    {
                        let error = error.context(&format!(
                            "capture request {} verification failed",
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
                        updates.push(Update::SourceCaptureRecovered {
                            capture_generation,
                            request: receipt.request,
                            stored: receipt.stored,
                        });
                        updates.push(Update::SourceCaptureRecoveryFailed {
                            capture_generation,
                            error: error.into(),
                        });
                    } else if let Some(stored) = receipt.stored {
                        updates.push(Update::SourceCaptureRevalidated {
                            capture_generation,
                            request: receipt.request,
                            stored,
                        });
                    } else {
                        updates.push(Update::SourceCaptureRecoveryFailed {
                            capture_generation,
                            error: "saved capture receipt has no artifacts".into(),
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
