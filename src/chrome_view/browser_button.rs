// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn browser_button(label: impl Into<String>, intent: Option<ChromeIntent>) -> ChromeView {
    let label = label.into();
    browser_control(label.clone(), &label, intent)
}
