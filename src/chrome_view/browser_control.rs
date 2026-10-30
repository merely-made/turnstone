// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn browser_control(
    label: impl Into<String>,
    name: &str,
    intent: Option<ChromeIntent>,
) -> ChromeView {
    cambium::shell_control(
        label,
        name,
        cambium::ShellControl {
            disabled: intent.is_none(),
            ..Default::default()
        },
        move |_: &mut ChromeState, _: PointerClick| intent.clone(),
    )
}
