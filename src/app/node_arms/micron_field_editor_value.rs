// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(super) fn micron_field_editor_value(field: &nematic::micron::forms::Field) -> String {
    match field.kind {
        nematic::micron::syntax::FieldKind::Text { .. } => field.value().to_owned(),
        nematic::micron::syntax::FieldKind::Checkbox { .. }
        | nematic::micron::syntax::FieldKind::Radio { .. } => {
            if field.checked() { "on" } else { "off" }.to_string()
        },
    }
}
