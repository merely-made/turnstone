// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The scenario, parsed from `TURNSTONE_SCENARIO` (a
/// path). A parse error yields a stillborn scenario whose first `finish` reports
/// the failure — the harness learns WHY instead of timing out. `None` when the
/// env var is unset (the turnstone driver, or no driver, runs instead).
/// How many visits a locked session holds for the trail before it drops the
/// rest. Far beyond an ordinary sitting; a bound so a session left locked
/// for days cannot grow without limit.
pub(super) const PENDING_TRAIL_CAP: usize = 4096;
