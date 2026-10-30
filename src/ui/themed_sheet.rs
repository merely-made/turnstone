// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// `roles` (host rules bound to `var(--tabard-color-*)`) under tabard's one
/// presentation stylesheet: the palette's custom properties, then the roles,
/// then the exact authored rules. Without a stored presentation the mode's
/// built-in theme supplies the palette, so no host keeps a colour table.
pub(super) fn themed_sheet(
    appearance: &crate::shell_services::AppearanceConfig,
    roles: String,
) -> String {
    let builtin = tabard::ThemePresentation::AuthoredStylesheet(Vec::new());
    tabard::presentation_stylesheet(
        appearance.theme_presentation.as_ref().unwrap_or(&builtin),
        &appearance.tabard_mode(),
        tabard::theme::registry::THEME_ID_DEFAULT,
        &roles,
    )
}
