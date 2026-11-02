// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Map follows the application theme through pictograph's one adapter,
//! the same mapping Graphshell applies to its canvas.

use crate::shell_services::AppearanceConfig;
use mere::canvas::CanvasTheme;
use tabard::ThemePresentation;
use tabard::theme::registry::Mode;

#[derive(Default)]
pub(crate) struct MapThemeCache {
    key: Option<(ThemePresentation, Mode)>,
    theme: Option<CanvasTheme>,
}

impl MapThemeCache {
    pub(crate) fn resolve(&mut self, appearance: &AppearanceConfig) -> Option<CanvasTheme> {
        // Without a stored presentation the mode's built-in theme supplies the
        // ground, as it does for the chrome.
        let builtin = ThemePresentation::AuthoredStylesheet(Vec::new());
        let presentation = appearance.theme_presentation.as_ref().unwrap_or(&builtin);
        let mode = appearance.tabard_mode();
        let current = self
            .key
            .as_ref()
            .is_some_and(|(held, held_mode)| held == presentation && *held_mode == mode);
        if !current {
            let palette = tabard::presentation_palette(
                presentation,
                &mode,
                tabard::theme::registry::THEME_ID_DEFAULT,
            );
            self.theme = Some(CanvasTheme::from_palette(
                &palette.palette,
                &palette.gated_roles(),
            ));
            self.key = Some((presentation.clone(), mode));
        }
        self.theme
    }
}

/// The Map's clear colour: the theme's ground (sRGB) as the linear value the
/// render target takes, or white when no theme applies.
pub(crate) fn clear_color(theme: Option<CanvasTheme>) -> wgpu::Color {
    let Some(theme) = theme else {
        return wgpu::Color::WHITE;
    };
    let linear = |channel: f32| {
        f64::from(if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        })
    };
    let [r, g, b, a] = theme.ground();
    wgpu::Color {
        r: linear(r),
        g: linear(g),
        b: linear(b),
        a: f64::from(a),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::App;

    fn light_presentation() -> ThemePresentation {
        tabard::resolve_theme_choice(
            &Default::default(),
            &tabard::theme::choice::ThemeChoice::new(
                tabard::theme::registry::THEME_ID_DEFAULT,
                Some(tabard::theme::registry::Mode::Light),
            ),
        )
        .unwrap()
        .presentation
    }

    #[test]
    fn the_map_is_built_at_logical_size_and_wears_the_theme() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/a".into()));
        let pane = app.default_graph_pane();
        let mut chrome = app.shell_chrome_config().clone();
        chrome.layout_scales.insert(0, 2.0);
        chrome.appearance.theme_mode = crate::shell_services::ThemeMode::Light;
        chrome.appearance.theme_presentation = Some(light_presentation());
        app.set_shell_chrome_config(chrome);

        assert_eq!(app.pane_layout_scale(pane), 2.0);
        let (scene, _) = app.graph_pane_frame(pane, 1200, 800).unwrap();
        assert!(!scene.ops.is_empty());
        let canvas = app
            .graph_runtimes
            .canvas(app.graph_for_pane(pane).unwrap())
            .unwrap();
        assert_eq!(
            canvas.viewport().view,
            (600, 400),
            "physical size over the layout scale"
        );

        let ThemePresentation::Derived(light) = light_presentation() else {
            unreachable!()
        };
        let faces = canvas.derived_face_palette().colors();
        assert_eq!(faces[0], light.palette.primary.to_array());
        assert_eq!(faces[2], light.palette.tertiary.to_array());
        assert_ne!(
            clear_color(app.map_theme()),
            wgpu::Color::WHITE,
            "ground follows the theme"
        );
    }

    #[test]
    fn pointers_reach_the_map_through_the_same_scale() {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.test/a".into()));
        let pane = app.default_graph_pane();
        let member = app.graph_runtimes.focused_member().unwrap();
        let mut chrome = app.shell_chrome_config().clone();
        chrome.layout_scales.insert(0, 2.0);
        app.set_shell_chrome_config(chrome);
        app.graph_pane_frame(pane, 1200, 800).unwrap();
        app.graph_pane_select_member(pane, member);
        let (x, y) = app
            .graph_runtimes
            .canvas(app.graph_for_pane(pane).unwrap())
            .unwrap()
            .focused_screen_position()
            .expect("the opened node is on screen");
        assert_eq!(
            app.graph_pane_node_at_screen(pane, x * 2.0, y * 2.0),
            Some(member)
        );
    }
}
