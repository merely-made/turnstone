// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn helper_subject(owner: &Owner) -> servitor::Subject {
    servitor::Subject(
        *blake3::hash(format!("turnstone-public-example-helper-v1/{owner:?}").as_bytes())
            .as_bytes(),
    )
}
