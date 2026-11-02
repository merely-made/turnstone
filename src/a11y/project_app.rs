// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Project the whole application into one stitched [`UxTree`]: a window root
/// whose children are the chrome subtree and the frisket pane tree, with the
/// workbench tiling under its pane leaf and a document subtree under the
/// canvas leaf for every node with live content.
pub fn project_app(app: &App) -> UxTree {
    let mut no_contribution = |_| None;
    project_app_capturing(app, &mut None, &mut no_contribution)
}
