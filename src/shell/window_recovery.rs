// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// One physical window's participation in the existing shared-core barrier.
/// GPU resources stay in the host, never in this recovery record.
pub(super) struct WindowRecovery {
    pub(super) subscription: genet_render_host::RecoverySubscription,
    pub(super) detached_generation: Option<u64>,
    pub(super) surface_failure: Option<(u64, String)>,
}
