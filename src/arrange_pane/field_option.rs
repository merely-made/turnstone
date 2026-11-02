// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn field_option(field_id: FieldId, id: &str) -> Option<&'static str> {
    let Control::Choice { options, .. } = PhysicsForm::default().field(field_id).control else {
        return None;
    };
    options
        .into_iter()
        .map(|(known, _)| known)
        .find(|known| *known == id)
}
