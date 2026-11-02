// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn hosted(calls: &Calls) -> PeltContent<netrender::Scene> {
    PeltContent::from_surface(
        Box::new(Page {
            calls: calls.clone(),
            pointers: Default::default(),
            acquisitions: Default::default(),
            fail_offset: None,
            title: Some("Site".to_owned()),
        }),
        (640, 480),
        route(),
    )
}
