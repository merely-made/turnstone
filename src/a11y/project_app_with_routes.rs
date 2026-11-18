// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The projection and its action envelopes. The window publisher admits
/// only envelopes whose node appears in its actual update.
pub fn project_app_with_routes(app: &App) -> (UxTree, ActionEnvelopes) {
    project_app_with_routes_and_contributions(app, std::collections::HashMap::new())
}
