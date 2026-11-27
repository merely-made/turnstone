// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The one mapping from a surface theme to the sheet painter's roles.
//!
//! `sheet-leaf` paints a guest's vector sheet in three roles and names no theme; every pane
//! that paints one (the block pane, the Mod viewer) takes its colours from here, so a web page
//! and a native window read one mapping (mod projects D50).

use cartography::canvas::backdrop_surface::SurfaceTheme;
use sheet_leaf::{ColorF, SheetTheme};

fn colour(c: [f32; 4]) -> ColorF {
    ColorF {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    }
}

/// The roles from a surface theme. Ink is the theme's text, accent its root colour and muted
/// its guide colour.
pub fn from_surface(theme: &SurfaceTheme) -> SheetTheme {
    SheetTheme {
        ink: colour(theme.text),
        accent: colour(theme.root),
        muted: colour(theme.guide),
    }
}

/// The pane background of a surface theme, which text runs must read against.
pub fn ground_of(theme: &SurfaceTheme) -> ColorF {
    ColorF {
        r: theme.ground[0],
        g: theme.ground[1],
        b: theme.ground[2],
        a: 1.0,
    }
}

/// The surface theme a derived palette gives (the web page's `backdrop_theme` reads the same
/// roles): ground is the background, text the body text, root the secondary
/// colour, guide the dim text at 0.35 alpha.
pub fn surface_of(palette: &tabard::ModePalette) -> SurfaceTheme {
    macro_rules! c {
        ($s:expr) => {
            [
                $s.r as f32 / 255.0,
                $s.g as f32 / 255.0,
                $s.b as f32 / 255.0,
                1.0,
            ]
        };
    }
    let p = &palette.palette;
    let mut guide = c!(p.text_dim);
    guide[3] *= 0.35;
    let ground = c!(p.bg);
    SurfaceTheme {
        ground,
        text: c!(p.text),
        guide,
        category: c!(p.primary),
        root: c!(p.secondary),
        fifth: c!(p.tertiary),
        warning: c!(p.danger),
    }
}

#[cfg(all(test, feature = "web"))]
mod tests {
    use super::*;
    use sheet_leaf::{SheetLeaf, contrast_ratio};
    use vector_sheet::{Anchor, Face, Op, Paint, Role, VectorSheet};

    fn text_sheet(role: Role) -> VectorSheet {
        let text = |t: &str| Op::Text {
            x: 0.0,
            y: 0.0,
            size: 12.0,
            text: t.into(),
            anchor: Anchor::Start,
            face: Face::Reading,
            paint: Paint::new(role),
        };
        VectorSheet {
            width: 100.0,
            height: 20.0,
            ops: vec![text("Dm7"), text("G7"), text("Cmaj7"), text("1")],
        }
    }

    #[test]
    fn text_in_the_ink_and_accent_roles_meets_aa_on_every_theme_and_mode() {
        // The guest paints chord symbols and bar numbers in Role::Ink (and the selected event's
        // in Role::Accent). Whatever the host maps those roles to must read against the pane.
        let appearance = crate::appearance::Appearance::default();
        let mut worst = (f32::MAX, String::new());
        let mut checked = 0;
        for (id, _) in appearance.themes() {
            for mode in appearance.modes() {
                let Ok(candidate) = appearance.with_choice(crate::appearance::ThemeChoice::new(
                    id.clone(),
                    Some(mode.clone()),
                )) else {
                    continue;
                };
                let tabard::ThemePresentation::Derived(palette) =
                    candidate.resolved().presentation.clone()
                else {
                    continue;
                };
                let surface = surface_of(&palette);
                let ground = ground_of(&surface);
                let theme = from_surface(&surface);
                for role in [Role::Ink, Role::Accent] {
                    let leaf = SheetLeaf::new(text_sheet(role), theme)
                        .unwrap()
                        .on_ground(ground);
                    for run in leaf.texts() {
                        let ratio = contrast_ratio(run.colour, ground);
                        checked += 1;
                        if ratio < worst.0 {
                            worst = (
                                ratio,
                                format!("{id} {} {role:?} {:?}", mode.as_key(), run.text),
                            );
                        }
                        assert!(
                            ratio >= 4.5,
                            "{id} {} {role:?} text {:?} has contrast {ratio:.2}, under AA 4.5",
                            mode.as_key(),
                            run.text
                        );
                    }
                }
            }
        }
        assert!(
            checked >= 4 * 2 * 4,
            "the built-in themes were not walked: {checked}"
        );
        eprintln!(
            "lowest text contrast over {checked} runs: {:.2} ({})",
            worst.0, worst.1
        );
    }

    #[test]
    fn the_host_default_surface_theme_also_meets_aa() {
        let surface = SurfaceTheme::default();
        let ground = ColorF {
            r: surface.ground[0],
            g: surface.ground[1],
            b: surface.ground[2],
            a: 1.0,
        };
        let leaf = SheetLeaf::new(text_sheet(Role::Ink), from_surface(&surface))
            .unwrap()
            .on_ground(ground_of(&surface));
        for run in leaf.texts() {
            assert!(contrast_ratio(run.colour, ground) >= 4.5);
        }
    }
}
