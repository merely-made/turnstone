// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn surface_poll_interval_from_env() -> std::time::Duration {
    let millis = std::env::var("TURNSTONE_SURFACE_POLL_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=60_000).contains(value))
        .unwrap_or(16);
    std::time::Duration::from_millis(millis)
}
