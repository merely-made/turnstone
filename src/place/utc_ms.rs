// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Milliseconds since the epoch as ISO 8601 UTC, to the second.
pub fn utc_ms(at_ms: u64) -> String {
    i64::try_from(at_ms)
        .ok()
        .and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
        .map_or_else(
            || format!("{at_ms} ms"),
            |at| at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
        )
}
