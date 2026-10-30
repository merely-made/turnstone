// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn admit_recovered_core(
    status: genet_render_host::RecoveryStatus,
    errors: &mut Vec<String>,
) -> bool {
    match status {
        genet_render_host::RecoveryStatus::Ready { .. } => true,
        genet_render_host::RecoveryStatus::Failed { loss, error } => {
            let terminal = format!(
                "device-loss recovery failed generation={}: {error}",
                loss.generation
            );
            if !errors.contains(&terminal) {
                tracing::warn!(generation = loss.generation, %error, "shared render-core recovery failed");
                errors.push(terminal);
            }
            false
        },
        _ => false,
    }
}
