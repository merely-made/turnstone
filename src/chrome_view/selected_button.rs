// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn selected_button(
    label: impl Into<String>,
    intent: ChromeIntent,
    selected: bool,
) -> ChromeView {
    let label = label.into();
    cambium::shell_control(
        label.clone(),
        label,
        cambium::ShellControl {
            pressed: Some(selected),
            ..Default::default()
        },
        move |_: &mut ChromeState, _: PointerClick| intent.clone(),
    )
}
