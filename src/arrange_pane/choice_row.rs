// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn choice_row(
    id: &str,
    label: &str,
    options: Vec<(String, String)>,
    current: &str,
    intent: fn(String) -> ArrangeIntent,
) -> ArrangeView {
    let spec = spec(
        id,
        label,
        choice_control(options),
        SettingValue::Text(current.to_string()),
    );
    Box::new(setting_row(
        &spec,
        label,
        move |_state: &mut ArrangeState, value: SettingValue| match value {
            SettingValue::Text(id) => Some(intent(id)),
            _ => None,
        },
    ))
}
