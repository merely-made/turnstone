// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn menu_button(
    state: &ChromeState,
    label: String,
    name: &str,
    menu: ChromeMenu,
) -> ChromeView {
    cambium::shell_control(
        label,
        name,
        cambium::ShellControl {
            expanded: Some(state.menu == Some(menu)),
            ..Default::default()
        },
        move |state: &mut ChromeState, _: PointerClick| {
            state.menu = if state.menu == Some(menu) {
                None
            } else {
                Some(menu)
            };
            state.menu_selected = 0;
            state.documents_start = 0;
            ChromeIntent::MenuChanged
        },
    )
}
