// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! The host's painter for a guest's vector sheet.
//!
//! The guest names paint by theme role and never by colour; this leaf maps each role to the
//! host theme and pushes ordinary paint commands. Text runs are not painted here: a host
//! draws them as its own text over the leaf, in its fonts ([`SheetLeaf::texts`]).
//! A host paints the non-zero fill rule only, so a sheet that asks for another is refused
//! at construction instead of being drawn wrongly.

use pictograph::canvas::backdrop_surface::SurfaceTheme;
pub use sprigging::ColorF;
use sprigging::{
    Leaf, PaintCx, Path, Size, SizeHint, StrokeCap, StrokeJoin, StrokeStyle,
};
use vector_sheet::{Anchor, Cap, Face, FillRule, Join, Op, Paint, Role, Seg, VectorSheet};

/// The host's colours for the sheet's roles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SheetTheme {
    pub ink: ColorF,
    pub accent: ColorF,
    pub muted: ColorF,
}

impl SheetTheme {
    /// The roles from a surface theme. Ink is the theme's text, accent its root colour and
    /// muted its guide colour, so a web page and a native window read one mapping.
    pub fn from_surface(theme: &SurfaceTheme) -> Self {
        let colour = |c: [f32; 4]| ColorF { r: c[0], g: c[1], b: c[2], a: c[3] };
        Self { ink: colour(theme.text), accent: colour(theme.root), muted: colour(theme.guide) }
    }

    /// The pane background of a surface theme, which text runs must read against.
    pub fn ground_of(theme: &SurfaceTheme) -> ColorF {
        ColorF { r: theme.ground[0], g: theme.ground[1], b: theme.ground[2], a: 1.0 }
    }

    fn colour(&self, paint: &Paint) -> ColorF {
        let base = match paint.role {
            Role::Ink => self.ink,
            Role::Accent => self.accent,
            Role::Muted => self.muted,
        };
        ColorF {
            a: base.a * paint.alpha,
            ..base
        }
    }

    /// The colour a text run is drawn in: its role's colour when that reads on the ground at
    /// WCAG AA, otherwise ink. Accent and muted are mark colours (a guide line at a third of
    /// its alpha is meant to be faint); a guest that sets text in them still gets legible text.
    fn text_colour(&self, paint: &Paint, ground: Option<ColorF>) -> ColorF {
        let wanted = self.colour(paint);
        let Some(ground) = ground else { return wanted };
        if contrast_ratio(wanted, ground) >= MIN_TEXT_CONTRAST {
            wanted
        } else {
            self.ink
        }
    }
}

/// WCAG AA for body text.
pub const MIN_TEXT_CONTRAST: f32 = 4.5;

/// A text run for the host to draw in its own fonts.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub text: String,
    pub anchor: Anchor,
    pub face: Face,
    pub colour: ColorF,
}

impl TextRun {
    /// The run's colour as a CSS value. A host that draws the run as its own text must set
    /// this, or the run takes whatever colour its container inherits.
    pub fn css_colour(&self) -> String {
        let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!(
            "rgba({},{},{},{:.3})",
            byte(self.colour.r),
            byte(self.colour.g),
            byte(self.colour.b),
            self.colour.a.clamp(0.0, 1.0)
        )
    }
}

/// WCAG 2.x contrast ratio of `foreground` over an opaque `background`; the foreground's
/// alpha is composited over the background first.
pub fn contrast_ratio(foreground: ColorF, background: ColorF) -> f32 {
    fn linear(c: f32) -> f32 {
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    fn luminance(c: ColorF) -> f32 {
        0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    }
    let a = foreground.a.clamp(0.0, 1.0);
    let over = ColorF {
        r: foreground.r * a + background.r * (1.0 - a),
        g: foreground.g * a + background.g * (1.0 - a),
        b: foreground.b * a + background.b * (1.0 - a),
        a: 1.0,
    };
    let (l1, l2) = (luminance(over), luminance(background));
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

pub struct SheetLeaf {
    sheet: VectorSheet,
    theme: SheetTheme,
    ground: Option<ColorF>,
    dirty: bool,
}

impl SheetLeaf {
    pub fn new(sheet: VectorSheet, theme: SheetTheme) -> Result<Self, String> {
        sheet.validate().map_err(|e| e.to_string())?;
        if sheet.ops.iter().any(|op| {
            matches!(op, Op::Fill { rule, .. } if *rule != FillRule::NonZero)
        }) {
            return Err("This host paints the non-zero fill rule only".into());
        }
        Ok(Self {
            sheet,
            theme,
            ground: None,
            dirty: true,
        })
    }

    /// Names the pane background. Text runs are then drawn in a colour that reads on it at
    /// WCAG AA (their role's, else ink); without it they keep their role's colour as given.
    pub fn on_ground(mut self, ground: ColorF) -> Self {
        self.ground = Some(ground);
        self
    }

    pub fn size(&self) -> Size {
        Size {
            width: self.sheet.width,
            height: self.sheet.height,
        }
    }

    pub fn set_theme(&mut self, theme: SheetTheme) {
        if self.theme != theme {
            self.theme = theme;
            self.dirty = true;
        }
    }

    /// The sheet's text runs, painted by the host over the leaf.
    pub fn texts(&self) -> Vec<TextRun> {
        self.sheet
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Text {
                    x,
                    y,
                    size,
                    text,
                    anchor,
                    face,
                    paint,
                } => Some(TextRun {
                    x: *x,
                    y: *y,
                    size: *size,
                    text: text.clone(),
                    anchor: *anchor,
                    face: *face,
                    colour: self.theme.text_colour(paint, self.ground),
                }),
                _ => None,
            })
            .collect()
    }

    fn path(segs: &[Seg]) -> sprigging::PathData {
        let mut path = Path::new();
        for seg in segs {
            path = match *seg {
                Seg::Move(x, y) => path.move_to(x, y),
                Seg::Line(x, y) => path.line_to(x, y),
                Seg::Quad(a, b, x, y) => path.quad_to(a, b, x, y),
                Seg::Cubic(a, b, c, d, x, y) => path.cubic_to(a, b, c, d, x, y),
                Seg::Close => path.close(),
            };
        }
        path.build()
    }
}

impl Leaf for SheetLeaf {
    fn measure(&mut self, _known: SizeHint, _available: SizeHint) -> Size {
        self.size()
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        for op in &self.sheet.ops {
            match op {
                Op::Fill { path, paint, .. } => {
                    cx.fill_path(Self::path(&path.segs), self.theme.colour(paint));
                },
                Op::Stroke {
                    path,
                    width,
                    cap,
                    join,
                    paint,
                } => cx.stroke_path(
                    Self::path(&path.segs),
                    StrokeStyle {
                        color: self.theme.colour(paint),
                        width: *width,
                        cap: match cap {
                            Cap::Butt => StrokeCap::Butt,
                            Cap::Round => StrokeCap::Round,
                            Cap::Square => StrokeCap::Square,
                        },
                        join: match join {
                            Join::Miter => StrokeJoin::Miter,
                            Join::Round => StrokeJoin::Round,
                            Join::Bevel => StrokeJoin::Bevel,
                        },
                        dash: None,
                    },
                ),
                Op::Text { .. } => {},
            }
        }
        self.dirty = false;
    }
    fn paint_dirty(&self) -> bool {
        self.dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_sheet::{Path as SheetPath, Seg};

    const THEME: SheetTheme = SheetTheme {
        ink: ColorF { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
        accent: ColorF { r: 0.1, g: 0.4, b: 0.9, a: 1.0 },
        muted: ColorF { r: 0.5, g: 0.5, b: 0.5, a: 1.0 },
    };
    const GROUND: ColorF = ColorF { r: 0.97, g: 0.97, b: 0.95, a: 1.0 };

    fn sheet() -> VectorSheet {
        let ring = [
            SheetPath::rect(0.0, 0.0, 20.0, 20.0).segs,
            // Opposite winding: a counter under the non-zero rule.
            vec![
                Seg::Move(5.0, 5.0),
                Seg::Line(5.0, 15.0),
                Seg::Line(15.0, 15.0),
                Seg::Line(15.0, 5.0),
                Seg::Close,
            ],
        ]
        .concat();
        VectorSheet {
            width: 100.0,
            height: 40.0,
            ops: vec![
                Op::Fill { path: SheetPath { segs: ring }, rule: FillRule::NonZero, paint: Paint::new(Role::Ink) },
                Op::Stroke {
                    path: SheetPath { segs: vec![Seg::Move(0.0, 30.0), Seg::Line(100.0, 30.0)] },
                    width: 1.0, cap: Cap::Butt, join: Join::Miter,
                    paint: Paint::with_alpha(Role::Accent, 0.5),
                },
                Op::Text { x: 2.0, y: 2.0, size: 12.0, text: "Dm7".into(), anchor: Anchor::Start, face: Face::Reading, paint: Paint::new(Role::Muted) },
            ],
        }
    }

    #[test]
    fn paints_fills_and_strokes_in_theme_roles_and_leaves_text_to_the_host() {
        let mut leaf = SheetLeaf::new(sheet(), THEME).unwrap();
        assert!(leaf.paint_dirty());
        let mut cmds = Vec::new();
        let size = leaf.size();
        leaf.paint(&mut PaintCx::new(&mut cmds, size));
        assert!(!leaf.paint_dirty());
        assert_eq!(cmds.len(), 2, "a fill and a stroke; text is the host's");
        let texts = leaf.texts();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].text, "Dm7");
        // Without a named ground the role's colour is given as it is.
        assert_eq!(texts[0].colour, THEME.muted);
        // Muted grey on this ground is under AA, so a leaf that knows its ground uses ink.
        let grounded = SheetLeaf::new(sheet(), THEME).unwrap().on_ground(GROUND);
        assert_eq!(grounded.texts()[0].colour, THEME.ink);
        assert_eq!(size, Size { width: 100.0, height: 40.0 });
    }

    #[test]
    fn a_text_role_that_does_not_read_on_the_ground_falls_back_to_ink() {
        let ground = ColorF { r: 0.06, g: 0.08, b: 0.10, a: 1.0 };
        let dark = SheetTheme {
            ink: ColorF { r: 0.92, g: 0.92, b: 0.90, a: 1.0 },
            muted: ColorF { r: 0.5, g: 0.55, b: 0.5, a: 0.35 },
            accent: ColorF { r: 0.1, g: 0.15, b: 0.4, a: 1.0 },
        };
        for role in [Role::Ink, Role::Muted, Role::Accent] {
            let leaf = SheetLeaf::new(text_sheet(role), dark).unwrap().on_ground(ground);
            for run in leaf.texts() {
                assert!(contrast_ratio(run.colour, ground) >= MIN_TEXT_CONTRAST, "{role:?}");
            }
        }
        // A role that does read keeps its own colour.
        let bright = SheetTheme { accent: ColorF { r: 0.5, g: 0.8, b: 1.0, a: 1.0 }, ..dark };
        let leaf = SheetLeaf::new(text_sheet(Role::Accent), bright).unwrap().on_ground(ground);
        assert_eq!(leaf.texts()[0].colour, bright.accent);
    }

    #[test]
    fn a_compound_path_is_one_command_with_both_subpaths() {
        let mut leaf = SheetLeaf::new(sheet(), THEME).unwrap();
        let mut cmds = Vec::new();
        let size = leaf.size();
        leaf.paint(&mut PaintCx::new(&mut cmds, size));
        let sprigging::PaintCmd::DrawPath(item) = &cmds[0] else { panic!("fill is a path") };
        let moves = item
            .path
            .commands
            .iter()
            .filter(|c| format!("{c:?}").starts_with("MoveTo"))
            .count();
        assert_eq!(moves, 2, "the counter survives");
    }

    #[test]
    fn an_unsupported_fill_rule_is_refused() {
        let mut bad = sheet();
        if let Op::Fill { rule, .. } = &mut bad.ops[0] {
            *rule = FillRule::EvenOdd;
        }
        assert!(SheetLeaf::new(bad, THEME).is_err());
    }

    #[test]
    fn alpha_multiplies_the_role_colour_and_a_theme_change_repaints() {
        let mut leaf = SheetLeaf::new(sheet(), THEME).unwrap();
        let mut cmds = Vec::new();
        let size = leaf.size();
        leaf.paint(&mut PaintCx::new(&mut cmds, size));
        assert!(!leaf.paint_dirty());
        leaf.set_theme(THEME);
        assert!(!leaf.paint_dirty(), "same theme does not repaint");
        leaf.set_theme(SheetTheme { ink: THEME.accent, ..THEME });
        assert!(leaf.paint_dirty());
        let sprigging::PaintCmd::DrawPath(stroke) = &cmds[1] else { panic!() };
        assert!((stroke.stroke.as_ref().unwrap().color.a - 0.5).abs() < 1e-6);
    }

    /// The surface theme the web page builds from a derived palette (`web_appearance`'s
    /// `backdrop_theme`): ground is the background, text the body text, root the secondary
    /// colour, guide the dim text at 0.35 alpha.
    fn surface_of(palette: &tabard::ModePalette) -> (SurfaceTheme, ColorF) {
        macro_rules! c {
            ($s:expr) => {
                [$s.r as f32 / 255.0, $s.g as f32 / 255.0, $s.b as f32 / 255.0, 1.0]
            };
        }
        let p = &palette.palette;
        let mut guide = c!(p.text_dim);
        guide[3] *= 0.35;
        let ground = c!(p.bg);
        let theme = SurfaceTheme {
            ground,
            text: c!(p.text),
            guide,
            category: c!(p.primary),
            root: c!(p.secondary),
            fifth: c!(p.tertiary),
            warning: c!(p.danger),
        };
        (theme, ColorF { r: ground[0], g: ground[1], b: ground[2], a: 1.0 })
    }

    fn text_sheet(role: Role) -> VectorSheet {
        let text = |t: &str| Op::Text {
            x: 0.0, y: 0.0, size: 12.0, text: t.into(), anchor: Anchor::Start,
            face: Face::Reading, paint: Paint::new(role),
        };
        VectorSheet { width: 100.0, height: 20.0, ops: vec![text("Dm7"), text("G7"), text("Cmaj7"), text("1")] }
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
                let Ok(candidate) = appearance
                    .with_choice(crate::appearance::ThemeChoice::new(id.clone(), Some(mode.clone())))
                else { continue };
                let tabard::ThemePresentation::Derived(palette) = candidate.resolved().presentation.clone()
                else { continue };
                let (surface, ground) = surface_of(&palette);
                let theme = SheetTheme::from_surface(&surface);
                for role in [Role::Ink, Role::Accent] {
                    let leaf = SheetLeaf::new(text_sheet(role), theme).unwrap().on_ground(ground);
                    for run in leaf.texts() {
                        let ratio = contrast_ratio(run.colour, ground);
                        checked += 1;
                        if ratio < worst.0 {
                            worst = (ratio, format!("{id} {} {role:?} {:?}", mode.as_key(), run.text));
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
        assert!(checked >= 4 * 2 * 4, "the built-in themes were not walked: {checked}");
        eprintln!("lowest text contrast over {checked} runs: {:.2} ({})", worst.0, worst.1);
    }

    #[test]
    fn the_host_default_surface_theme_also_meets_aa() {
        let surface = SurfaceTheme::default();
        let ground = ColorF { r: surface.ground[0], g: surface.ground[1], b: surface.ground[2], a: 1.0 };
        let leaf = SheetLeaf::new(text_sheet(Role::Ink), SheetTheme::from_surface(&surface))
            .unwrap()
            .on_ground(SheetTheme::ground_of(&surface));
        for run in leaf.texts() {
            assert!(contrast_ratio(run.colour, ground) >= 4.5);
        }
    }

    #[test]
    fn contrast_ratio_matches_the_wcag_extremes_and_composites_alpha() {
        let black = ColorF { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
        let white = ColorF { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
        assert!((contrast_ratio(black, white) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-6);
        // Black on a dark ground is the failure this guards: the old host drew guest text in
        // whatever colour it inherited.
        let ground = ColorF { r: 0.05, g: 0.07, b: 0.09, a: 1.0 };
        assert!(contrast_ratio(black, ground) < 1.5);
        // Fully transparent paint is the ground itself.
        assert!((contrast_ratio(ColorF { a: 0.0, ..white }, ground) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_text_run_names_its_colour_in_css() {
        let leaf = SheetLeaf::new(text_sheet(Role::Ink), THEME).unwrap();
        assert_eq!(leaf.texts()[0].css_colour(), "rgba(26,26,26,1.000)");
        let faded = TextRun {
            x: 0.0, y: 0.0, size: 12.0, text: "x".into(), anchor: Anchor::Start, face: Face::Reading,
            colour: ColorF { a: 0.5, ..THEME.accent },
        };
        assert_eq!(faded.css_colour(), "rgba(26,102,230,0.500)");
    }
}
