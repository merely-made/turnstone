// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn requested_gpu_sync() -> Result<DiagnosticGpuSync, SurfaceError> {
    fn option(name: &str) -> Result<Option<String>, SurfaceError> {
        match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(_) => Err(spawn_error(format!("{name} must be Unicode"))),
        }
    }
    let mode = option("TURNSTONE_SERVO_GPU_SYNC")?;
    let timeout = option("TURNSTONE_SERVO_GPU_SYNC_TIMEOUT_MS")?
        .map(|value| value.parse::<u64>())
        .transpose()
        .map_err(|_| spawn_error("Servo GPU diagnostic timeout must be positive milliseconds"))?
        .unwrap_or(5000);
    if timeout == 0 {
        return Err(spawn_error(
            "Servo GPU diagnostic timeout must be positive milliseconds",
        ));
    }
    parse_gpu_sync(
        mode.as_deref().unwrap_or("existing"),
        std::time::Duration::from_millis(timeout),
    )
}
