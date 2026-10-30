// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn field_label(field_id: FieldId, id: &str) -> String {
    if field_id == FieldId::PhysicsProfile && id == "custom" {
        return "custom".to_string();
    }
    match PhysicsForm::default().field(field_id).control {
        Control::Choice { options, .. } => options
            .into_iter()
            .find(|(known, _)| *known == id)
            .map(|(_, label)| label.to_string())
            .unwrap_or_else(|| id.to_string()),
        _ => id.to_string(),
    }
}
