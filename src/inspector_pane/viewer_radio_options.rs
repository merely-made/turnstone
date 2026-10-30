// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The viewer options as Cambium radio options: unselectable engines stay
/// listed (and keep a saved selection) but are disabled.
pub(super) fn viewer_radio_options(
    options: &[crate::inspector_controls::ViewerOption],
) -> Vec<RadioOption> {
    options
        .iter()
        .map(|option| {
            RadioOption::new(option.label.clone())
                .disabled(!option.selectable)
                .with_attr(
                    "data-engine-id",
                    option.viewer.clone().unwrap_or_else(|| "auto".into()),
                )
        })
        .collect()
}
