// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse_gpu_sync(
    mode: &str,
    timeout: std::time::Duration,
) -> Result<DiagnosticGpuSync, SurfaceError> {
    // These explicit diagnostics preserve the existing production policy.
    // Native runners guard the process because GL producer completion has no timeout.
    match mode {
        "existing" => Ok(DiagnosticGpuSync::Existing),
        "producer" => Ok(DiagnosticGpuSync::ProducerCompletion),
        "normalization" => Ok(DiagnosticGpuSync::NormalizationCompletion { timeout }),
        "both" => Ok(DiagnosticGpuSync::Both { timeout }),
        _ => Err(spawn_error(
            "TURNSTONE_SERVO_GPU_SYNC must be existing, producer, normalization or both",
        )),
    }
}
