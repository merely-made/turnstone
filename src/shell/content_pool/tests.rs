// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::surface::{Rect, SurfaceKind};
use pelt_core::{PeltController, PeltControllerConfig, PeltRoute, PeltRouteSource, PeltRouteState};

fn reader<'a>(
    contents: &'a NodeDocuments,
    key: &ContentKey,
) -> &'a mere_document_lanes::ReaderDocumentSession {
    contents
        .pool
        .content(key)
        .unwrap()
        .document()
        .unwrap()
        .session()
        .as_any_ref()
        .downcast_ref()
        .unwrap()
}

#[path = "tests/hidden_finite_livery_work_schedules_ticks_until_settled_cases.rs"]
mod hidden_finite_livery_work_schedules_ticks_until_settled_cases;
#[path = "tests/settled_timer_deadline_survives_early_events_and_uses_cases.rs"]
mod settled_timer_deadline_survives_early_events_and_uses_cases;
