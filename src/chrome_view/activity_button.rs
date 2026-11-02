// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn activity_button(
    mark: &str,
    label: &str,
    name: &str,
    intent: ChromeIntent,
    pressed: Option<bool>,
) -> ChromeView {
    cambium::shell_activity_control(
        mark,
        label,
        name,
        cambium::ShellControl {
            pressed,
            ..Default::default()
        },
        move |_: &mut ChromeState, _: PointerClick| intent.clone(),
    )
}
