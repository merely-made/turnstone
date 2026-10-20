// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's role bindings for the shared outer pane frame. Styles remain
//! input to the same retained layout that paints and hit-tests the furniture.
use tabard::{FontRole, ThemePresentation, Typography};

const INTERFACE: &str = ".frisket-tab,.frisket-label,.frisket-close";

pub(crate) fn interface_font_properties(
    fonts: &tabard::FontPreferences,
    installed: &[String],
) -> String {
    let fonts = tabard::FontPreferences(
        fonts
            .0
            .iter()
            .filter(|(role, _)| **role == FontRole::Interface)
            .map(|(&role, families)| (role, families.clone()))
            .collect(),
    );
    let defaults = [(
        FontRole::Interface,
        tabard::FontFamily::new("sans-serif").unwrap(),
    )]
    .into();
    fonts.css_custom_properties(installed, &defaults)
}

pub(crate) fn stylesheet(
    presentation: Option<&ThemePresentation>,
    typography: &Typography,
    interface_font_properties: &str,
    zoom: f32,
) -> String {
    let zoom = if zoom.is_finite() {
        zoom.clamp(0.5, 3.0)
    } else {
        1.0
    };
    let fallback = tabard::resolve_theme_choice(
        &tabard::theme::registry::ThemeRegistry::default(),
        &tabard::theme::choice::ThemeChoice::default(),
    )
    .expect("valid Tabard default");
    let palette = match presentation {
        Some(ThemePresentation::Derived(palette)) => palette,
        _ => match &fallback.presentation {
            ThemePresentation::Derived(palette) => palette,
            _ => unreachable!("built-in default is derived"),
        },
    };
    // Automatic header height accommodates authored family/size/leading.
    // UI zoom scales host defaults; explicit authored sizes retain CSS authority.
    let mut sheet = format!(
        "{}\n{}\n{}\n\
        .frisket-tabbar {{ background:var(--tabard-color-surface); height:auto; flex-basis:auto; min-height:{:.2}px; padding:{:.2}px {:.2}px 0; }}\n\
        .frisket-tab {{ color:var(--tabard-color-text); background:var(--tabard-color-surface-2); padding:{:.2}px {:.2}px; font-size:{:.2}px; line-height:1.2; }}\n\
        .frisket-tab.active {{ color:var(--tabard-color-text-header); background:var(--tabard-color-bg); }}\n\
        .frisket-close {{ color:var(--tabard-color-text-dim); font-size:{:.2}px; width:{:.2}px; flex-basis:{:.2}px; min-height:{:.2}px; height:auto; padding:{:.2}px 0; box-sizing:border-box; }}\n\
        .frisket-divider {{ flex-basis:1px; background:var(--tabard-color-surface-2); border:0; box-sizing:border-box; }}\n\
        .frisket-divider[aria-orientation=vertical] {{ border-left-width:1px; }}\n\
        .frisket-divider[aria-orientation=horizontal] {{ border-top-width:1px; }}\n\
        .frisket-content {{ background:transparent; }}\n\
        .turnstone-window-root {{ background:var(--tabard-color-bg); }}\n",
        cambium::FRISKET_CSS,
        cambium::WORKSPACE_CSS,
        palette.css_custom_properties(),
        28.0 * zoom,
        2.0 * zoom,
        2.0 * zoom,
        4.0 * zoom,
        8.0 * zoom,
        13.0 * zoom,
        13.0 * zoom,
        20.0 * zoom,
        20.0 * zoom,
        20.0 * zoom,
        2.0 * zoom,
    );
    sheet.push_str("[data-frame-focus=true]{outline:2px solid var(--tabard-color-primary);outline-offset:-2px;}\n");
    // Only Interface is adopted here. Reading/Code belong to pane bodies.
    // Keep all variables available to explicit CSS, but bind only Interface.
    sheet.push_str(&typography.css_custom_properties());
    sheet.push_str(&typography.role_css(FontRole::Interface, INTERFACE));
    if !interface_font_properties.is_empty() {
        sheet.push_str(interface_font_properties);
        sheet.push_str(&format!(
            "{INTERFACE}{{font-family:var(--tabard-font-interface);}}\n"
        ));
    }
    // Preserve the exact authored rules and their final cascade position.
    if let Some(ThemePresentation::AuthoredStylesheet(rules)) = presentation {
        // Shared resolution prepends generated theme font properties. They
        // have already been bound above; app family requests override those
        // defaults, while the actual authored CSS keeps final authority.
        let generated = typography.css_custom_properties();
        let skip = usize::from(!generated.is_empty() && rules.first() == Some(&generated));
        sheet.push_str(&rules[skip..].join("\n"));
    }
    sheet
}
