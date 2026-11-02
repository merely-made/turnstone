// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Application-owned chrome and pane structure for the composed AccessKit forest.
//!
//! The app projection is pure over `App`. Its spawn outline remains a fallback
//! for lanes without live source ownership. The shell replaces non-Reader
//! document outlines with controller projections and mounts contributed DOMs
//! through native Mere grafts. Platform publication and scenario observations
//! read the same forest in root/children order. Source support declarations
//! remain intact, including Partial and unavailable semantics.

use crate::panes::PaneContent;
use accesskit::{Node, Role};
use uxtree::{UxTree, node_id_for_path, stitch};

use crate::app::App;
use crate::content::{NodeContent, OutlineFact};

/// Where a screen-reader action on a projected node lands in the app.
///
/// The projection hashes paths into `NodeId`s, which is one-way, so routing
/// is a table built beside the tree rather than parsed back out of it. The
/// meerkat-era checklist called this the route table; this is that idea,
/// rebuilt where the projection actually lives now.
#[derive(Clone, Debug, PartialEq)]
pub enum A11yRoute {
    /// Exact shared frame control; the publishing window owns its route table.
    Frame(crate::frame_controls::FrameRoute),
    /// The chrome: open the omnibar in its find lane.
    OpenOmnibar,
    /// A chrome control: run this action through the update spine, exactly
    /// as its click does (a palette row's commit, its Keep or Drop).
    Action(crate::action::Action),
    /// A frozen-projection instance: select that member in the graph pane, so
    /// a reader who found a node by name can make it the app's selection.
    SelectMember {
        pane: crate::panes::PaneId,
        member: uuid::Uuid,
    },
    /// A retained product surface: route the original action, unchanged, to
    /// the DOM node that Genet projected for this pane.
    Contributed {
        pane: crate::panes::PaneId,
        generation: u64,
        node: genet_scripted_dom::NodeId,
    },
}

/// Routes for every node an assistive action may target.
pub type A11yRoutes = std::collections::HashMap<accesskit::NodeId, A11yRoute>;

#[cfg(test)]
#[path = "a11y/route_table.rs"]
mod route_table;

#[cfg(test)]
#[path = "a11y/bridge_integrity.rs"]
mod bridge_integrity;

#[cfg(test)]
#[path = "a11y/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "a11y/reading_order_tests.rs"]
mod reading_order_tests;

#[cfg(test)]
#[path = "a11y/micron_form_tests.rs"]
mod micron_form_tests;

#[cfg(test)]
#[path = "a11y/lens_window_tests.rs"]
mod lens_window_tests;

#[path = "a11y/project_app.rs"]
mod project_app;
pub use project_app::project_app;
#[path = "a11y/lens_window_label.rs"]
mod lens_window_label;
pub(crate) use lens_window_label::lens_window_label;
#[path = "a11y/project_lens_with_contributions.rs"]
mod project_lens_with_contributions;
pub(crate) use project_lens_with_contributions::project_lens_with_contributions;
#[path = "a11y/project_app_capturing.rs"]
mod project_app_capturing;
use project_app_capturing::project_app_capturing;
#[path = "a11y/a11y_lines.rs"]
mod a11y_lines;
pub use a11y_lines::a11y_lines;
#[path = "a11y/tree_lines.rs"]
mod tree_lines;
pub use tree_lines::tree_lines;
#[path = "a11y/forest_lines.rs"]
mod forest_lines;
pub(crate) use forest_lines::forest_lines;
#[path = "a11y/matches_lines.rs"]
mod matches_lines;
pub(crate) use matches_lines::matches_lines;
#[path = "a11y/host_input_focus.rs"]
mod host_input_focus;
pub(crate) use host_input_focus::host_input_focus;
#[path = "a11y/focus_host_input.rs"]
mod focus_host_input;
pub(crate) use focus_host_input::focus_host_input;
#[path = "a11y/project_frozen_projection.rs"]
mod project_frozen_projection;
use project_frozen_projection::project_frozen_projection;
#[path = "a11y/omnibar_rows.rs"]
mod omnibar_rows;
use omnibar_rows::omnibar_rows;
#[path = "a11y/project_chrome.rs"]
mod project_chrome;
use project_chrome::project_chrome;
#[path = "a11y/toolbar_actions.rs"]
mod toolbar_actions;
use toolbar_actions::toolbar_actions;
#[path = "a11y/project_live_document.rs"]
mod project_live_document;
use project_live_document::project_live_document;
#[path = "a11y/outline_role.rs"]
mod outline_role;
use outline_role::outline_role;
#[path = "a11y/project_app_with_routes.rs"]
mod project_app_with_routes;
pub use project_app_with_routes::project_app_with_routes;
#[path = "a11y/project_app_with_routes_and_contributions.rs"]
mod project_app_with_routes_and_contributions;
pub(crate) use project_app_with_routes_and_contributions::project_app_with_routes_and_contributions;
#[path = "a11y/apply_route.rs"]
mod apply_route;
pub(crate) use apply_route::apply_route;
