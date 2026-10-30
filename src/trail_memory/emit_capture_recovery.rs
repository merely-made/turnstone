// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn emit_capture_recovery(
    state: &mut Option<(FjallStore, BrowsingMemory)>,
    custody: &SourceCustody,
    directory: &Path,
    capture_generation: uuid::Uuid,
    out: &Emitter<Update>,
) {
    let Some(custody_epoch) = custody.snapshot() else {
        return;
    };
    let Some((store, _)) = state.as_mut() else {
        return;
    };
    let mut updates = Vec::new();
    match source_capture_receipts::recover_receipts(store) {
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
                    if let Err(error) = source_capture_receipts::verify_receipt(store, &receipt) {
                        let id = receipt.request.id;
                        updates.push(Update::SourceCaptureRecovered {
                            capture_generation,
                            request: receipt.request,
                            stored: receipt.stored,
                        });
                        updates.push(Update::SourceCaptureRecoveryFailed {
                            capture_generation,
                            error: format!("capture request {id} verification failed: {error}"),
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
            error,
        }),
    }
    for update in updates {
        if !custody.remains_current(Some(custody_epoch)) {
            break;
        }
        out.emit(update);
    }
}
