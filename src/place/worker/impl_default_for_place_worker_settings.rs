// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for PlaceWorkerSettings {
    fn default() -> Self {
        Self {
            authority_clock: AuthorityClock::SystemTime,
            knot_root: crate::knot_authoring::env_path("TURNSTONE_KNOT_ROOT"),
            knot_max_source_bytes: std::env::var("TURNSTONE_KNOT_MAX_BYTES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_KNOT_MAX_SOURCE_BYTES),
            capture_library: crate::place::captured_collection::LocalCaptureLibrary::default(),
            #[cfg(feature = "peers")]
            retention: MootRetentionSettings {
                revision: PolicyRevision(Digest::blake3(b"turnstone.place.offline-retention.v1")),
                availability: AvailabilityPolicy {
                    promised_floor: KeepBound::Forever,
                },
                erasure: ErasurePolicy {
                    history_ceiling: KeepBound::Forever,
                },
            },
        }
    }
}
