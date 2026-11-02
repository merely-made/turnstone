// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The projection flattened to "role: label" lines for the observation
/// snapshot (what `assert a11y` matches). Values fold into the line when a
/// node has one (the omnibar's text, a link's target).
pub fn a11y_lines(app: &App) -> Vec<String> {
    tree_lines(&project_app(app))
}
