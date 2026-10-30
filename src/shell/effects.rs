// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The effect runner and session save: the one place effects meet ports.
//!
//! `App::update` returns effects; every port call the app asked for happens
//! here, and every answer comes back as a typed `Update`. Nothing else in the
//! shell talks to a port.

use super::{NodeSessions, NodeSurfaces};
use std::sync::mpsc::Receiver;

use fetch::{FetchCommand, FetchUpdate};
use inker::{
    DocumentFindQuery, EngineProfileBinding, SessionClick, SessionSpawnRequest, SurfaceSpawnRequest,
};

use crate::action::{Action, Effect, Update};
use crate::browse;
use crate::panes::PaneContent;
use crate::session;

use super::Shell;

#[derive(Debug, PartialEq, Eq)]
enum ContentSpawnDispatch {
    PendingBrowser,
    Browser,
    Session,
}

/// A held body and its optional newly acquired source stay with one spawn.
struct PreparedContentController {
    node: uuid::Uuid,
    url: String,
    config: pelt_core::PeltControllerConfig,
    source: Option<crate::content::FetchedDocument>,
}

struct OpenedContentController {
    controller: pelt_core::PeltController<netrender::Scene>,
    source: Option<Update>,
}

#[cfg(test)]
#[path = "local_micron_source_tests.rs"]
mod local_micron_source_tests;

#[cfg(all(test, feature = "nomadnet"))]
#[path = "nomadnet_receiving_tests.rs"]
mod nomadnet_receiving_tests;

#[cfg(test)]
#[path = "smolweb_receiving_tests.rs"]
mod smolweb_receiving_tests;

#[cfg(test)]
#[path = "effects/content_spawn_dispatch_tests.rs"]
mod content_spawn_dispatch_tests;

#[cfg(all(test, windows))]
#[path = "effects/browser_profile_path_tests.rs"]
mod browser_profile_path_tests;

/// How many recalled pages the omnibar asks for. The lane sits below the
/// node and go rows, so a handful is what there is room to read.
const RECALL_ROW_LIMIT: usize = 5;

#[cfg(test)]
#[path = "effects/reader_lineage_tests.rs"]
mod reader_lineage_tests;

#[cfg(test)]
#[path = "effects/retained_page_zoom_tests.rs"]
mod retained_page_zoom_tests;

#[path = "effects/dispatch_content_spawn.rs"]
mod dispatch_content_spawn;
use dispatch_content_spawn::dispatch_content_spawn;
#[path = "effects/replace_fetched_smolweb_body.rs"]
mod replace_fetched_smolweb_body;
use replace_fetched_smolweb_body::replace_fetched_smolweb_body;
#[path = "effects/content_spawn_is_current.rs"]
mod content_spawn_is_current;
use content_spawn_is_current::content_spawn_is_current;
#[path = "effects/prepare_content_controller.rs"]
mod prepare_content_controller;
use prepare_content_controller::prepare_content_controller;
#[path = "effects/open_prepared_content_controller.rs"]
mod open_prepared_content_controller;
use open_prepared_content_controller::open_prepared_content_controller;
#[path = "effects/browser_profile_path.rs"]
mod browser_profile_path;
pub(super) use browser_profile_path::browser_profile_path;
#[path = "effects/recall_sources.rs"]
mod recall_sources;
use recall_sources::recall_sources;
#[path = "effects/app_find_model.rs"]
mod app_find_model;
pub(super) use app_find_model::app_find_model;
#[path = "effects/app_document_capabilities.rs"]
mod app_document_capabilities;
pub(super) use app_document_capabilities::app_document_capabilities;
#[path = "effects/app_page_zoom_facts.rs"]
mod app_page_zoom_facts;
pub(super) use app_page_zoom_facts::app_page_zoom_facts;
#[path = "effects/seed_page_scale.rs"]
mod seed_page_scale;
use seed_page_scale::seed_page_scale;
#[path = "effects/apply_page_scale.rs"]
mod apply_page_scale;
use apply_page_scale::apply_page_scale;
#[path = "effects/replay_page_scale.rs"]
mod replay_page_scale;
use replay_page_scale::replay_page_scale;
#[path = "effects/apply_retained_page_scale.rs"]
mod apply_retained_page_scale;
use apply_retained_page_scale::apply_retained_page_scale;
#[path = "effects/replay_retained_page_scale.rs"]
mod replay_retained_page_scale;
use replay_retained_page_scale::replay_retained_page_scale;
#[path = "effects/project_reader_lineage_facet.rs"]
mod project_reader_lineage_facet;
use project_reader_lineage_facet::project_reader_lineage_facet;
#[path = "effects/read_place_artifact.rs"]
mod read_place_artifact;
#[path = "effects/shell_effect_control_content.rs"]
mod shell_effect_control_content;
#[path = "effects/shell_effect_open_theme_workshop.rs"]
mod shell_effect_open_theme_workshop;
#[path = "effects/shell_effect_spawn_content.rs"]
mod shell_effect_spawn_content;
#[path = "effects/shell_effect_trash_session.rs"]
mod shell_effect_trash_session;
#[path = "effects/shell_refresh_diagnostic_inspection.rs"]
mod shell_refresh_diagnostic_inspection;
#[path = "effects/shell_refuse_visit.rs"]
mod shell_refuse_visit;
use read_place_artifact::read_place_artifact;
