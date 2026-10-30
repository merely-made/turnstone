// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The projection and its route table, built in one pass.
///
/// Routes may name nodes the current tree does not contain (the frozen pane
/// closed, say); that is harmless, because the platform can only request
/// actions on nodes it was served. The reverse would not be harmless, which
/// is what the coverage test below pins.
pub fn project_app_with_routes(app: &App) -> (UxTree, A11yRoutes) {
    project_app_with_routes_and_contributions(app, std::collections::HashMap::new())
}
