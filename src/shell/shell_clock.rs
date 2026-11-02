// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Every retained document controller uses the same monotonic shell epoch.
pub(crate) struct ShellClock(pub(super) std::time::Instant);
