// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn emit_sealed_capture_recovery(
    store: &mut FjallStore,
    custody: &SourceCustody,
    directory: &Path,
    capture_generation: uuid::Uuid,
    out: &Emitter<Update>,
) {
    let Some(custody_epoch) = custody.snapshot() else {
        return;
    };
    let mut updates = Vec::new();
    match sealed_capture::receipts(store, custody.sealer.as_ref()) {
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
                        if sealed_capture::verify(
                            store,
                            custody.sealer.as_ref(),
                            &receipt.request,
                            &stored,
                        )
                        .is_ok()
                        {
                            updates.push(Update::SourceCaptureRevalidated {
                                capture_generation,
                                request: receipt.request,
                                stored,
                            });
                        } else {
                            updates.push(Update::SourceCaptureRecoveryFailed {
                                capture_generation,
                                error: "sealed capture verification refused".into(),
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
