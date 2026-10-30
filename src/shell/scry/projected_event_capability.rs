// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn projected_event_capability(
    status: WebFeatureStatus,
    projection: &str,
) -> WebFeatureStatus {
    match status {
        WebFeatureStatus::Supported => WebFeatureStatus::Partial {
            detail: projection.into(),
        },
        WebFeatureStatus::Partial { detail } => WebFeatureStatus::Partial {
            detail: format!("{detail}; {projection}"),
        },
        unsupported => unsupported,
    }
}
