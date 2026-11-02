// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn duration_nanoseconds(duration: std::time::Duration, field: &str) -> u64 {
    u64::try_from(duration.as_nanos())
        .unwrap_or_else(|_| panic!("{field} exceeds the V1 nanosecond range"))
}
