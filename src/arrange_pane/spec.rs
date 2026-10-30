// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn spec(
    id: &str,
    label: &str,
    control: SettingControl,
    value: SettingValue,
) -> SettingSpec {
    SettingSpec {
        id: id.to_string(),
        label: label.to_string(),
        scope: SettingScope::SessionDataspace,
        movement: SettingMovement::LocalOnly,
        mutability: SettingMutability::Live,
        security: SettingSecurity::Ordinary,
        control,
        value,
    }
}
