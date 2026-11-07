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
}

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

pub struct SheetLeaf {
    sheet: VectorSheet,
    theme: SheetTheme,
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
            dirty: true,
        })
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
                    colour: self.theme.colour(paint),
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
        assert_eq!(texts[0].colour, THEME.muted);
        assert_eq!(size, Size { width: 100.0, height: 40.0 });
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
}
