// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Graphshell's presentation host.
//!
//! G1 adds a native semantic view over the portable client state. Networking,
//! product models, and source authority remain injected at the edge.

#[cfg(feature = "moot-resources")]
pub mod moot_resources;
#[cfg(feature = "query-views")]
pub mod query_views;
#[cfg(feature = "retained-content")]
pub mod retained_content;

#[cfg(feature = "web")]
pub mod access;
#[cfg(feature = "web")]
pub mod access_inspection;
#[cfg(feature = "web")]
pub mod app;
#[cfg(feature = "web")]
pub mod appearance;
#[cfg(feature = "web")]
pub mod browser_storage;
#[cfg(feature = "web")]
#[cfg(test)]
mod canvas_physics_tests;
#[cfg(feature = "web")]
pub mod canvas_reader;
#[cfg(feature = "applets")]
pub mod capsule_applet;
#[cfg(feature = "web")]
pub mod capture;
#[cfg(all(test, feature = "web"))]
mod capture_resources;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod carriage;
#[cfg(any(feature = "native", feature = "web"))]
pub mod distillery_w1;
#[cfg(feature = "web")]
pub mod field_inspection;
#[cfg(feature = "web")]
pub mod forme_session;
#[cfg(feature = "web")]
pub mod forme_workspace;
#[cfg(feature = "web")]
pub mod frame_budget;
#[cfg(feature = "web")]
pub mod handlers;
#[cfg(feature = "web")]
pub mod host_dataset_view;
#[cfg(feature = "web")]
pub mod host_history_view;
#[cfg(feature = "web")]
pub mod host_presenter;
#[cfg(feature = "web")]
pub mod link_inspection;
#[cfg(feature = "web")]
pub mod promoted_links;
#[cfg(feature = "web")]
pub mod local_edit;
#[cfg(feature = "web")]
pub mod local_intake;
#[cfg(any(feature = "native", feature = "web"))]
pub mod local_projection;
#[cfg(feature = "web")]
pub mod local_scene;
#[cfg(feature = "web")]
pub mod mere_host;
#[cfg(feature = "web")]
mod mere_host_fixture;
#[cfg(feature = "web")]
pub mod node_presentation;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod personal_sync;
#[cfg(feature = "web")]
pub mod presentation_binding;
#[cfg(feature = "web")]
pub mod presentation_editor;
#[cfg(feature = "web")]
pub mod product;
#[cfg(feature = "web")]
pub mod remote_board;
#[cfg(feature = "web")]
pub mod resident_host;
#[cfg(feature = "web")]
pub mod responsive_links;
#[cfg(feature = "web")]
pub mod retained_policy;
#[cfg(feature = "web")]
pub mod session_item;
#[cfg(feature = "web")]
pub mod speed_bar;
#[cfg(feature = "web")]
pub mod viewer_commands;

#[cfg(any(feature = "native", feature = "web"))]
pub mod practice_disclosure;
#[cfg(any(feature = "native", feature = "web"))]
pub mod practice_workspace;
#[cfg(any(feature = "native", feature = "web"))]
pub mod projection_compare;
#[cfg(any(feature = "native", feature = "web"))]
pub mod projection_compile;
#[cfg(feature = "web")]
pub mod projection_dynamics_compare;
#[cfg(feature = "web")]
pub mod projection_dynamics_receipt;
/// Receipt ingest: a scenario-receipt directory becomes personal-graph facts.
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod receipts;
#[cfg(feature = "web")]
pub mod transfer;
#[cfg(feature = "web")]
pub mod transfer_endpoint;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod transfer_offer;
#[cfg(all(feature = "webrtc-browser", target_arch = "wasm32"))]
pub mod webrtc_browser;

pub use chirograph as protocol;
pub use graphshell_client as client;
pub use graphshell_endpoint as endpoint;
