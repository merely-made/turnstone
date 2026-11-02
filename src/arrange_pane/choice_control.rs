// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn choice_control(
    options: impl IntoIterator<Item = (String, String)>,
) -> SettingControl {
    SettingControl::Choice {
        options: options
            .into_iter()
            .map(|(value, label)| SettingOption { value, label })
            .collect(),
    }
}
