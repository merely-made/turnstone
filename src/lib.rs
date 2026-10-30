// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone application core and desktop host.
//!
//! The library boundary exposes the existing read model, action reducer, and
//! remote projection adapter. Platform handles remain in [`shell`].

pub mod a11y;
pub mod action;
#[cfg(feature = "workshop")]
pub mod alembic_workshop;
pub mod app;
mod appearance;
mod arrange_pane;
pub mod behaviors;
mod browse;
mod cambium_pane;
pub mod chrome_layout;
mod chrome_view;
#[cfg(feature = "wasm")]
mod component;
mod content;
mod content_classes;
mod content_tags;
mod contributed_a11y;
pub mod contributed_surface;
pub mod cookie_custody;
mod custody_binding;
mod denizen;
pub mod focus_card;
mod forme_context;
mod map_theme;
pub mod reading_view;
pub mod scope_navigation;

mod answer_source_contract;
mod authoring_address;
pub mod client_limits;
mod device_receipts_pane;
mod device_receipts_service;
pub mod diagnostic_observations;
#[cfg(feature = "inference")]
pub mod distillery_installed_surface;
pub mod document_find;
mod document_pump;
mod download;
mod feed;
mod foreign_a11y;
mod frame_a11y;
mod frame_controls;
mod frame_style;
pub mod frame_timing;
mod frozen_projection_pane;
mod gemini_identity;
mod gemini_trust;
mod graph_facets;
pub mod graph_scenes;
mod host_journal;
mod identity;
mod inference_contract;
#[cfg(feature = "inference")]
mod inference_service;
#[cfg(not(feature = "inference"))]
#[path = "disabled/inference_service.rs"]
mod inference_service;
#[cfg(feature = "inference")]
pub mod inference_surface;
#[cfg(not(feature = "inference"))]
#[path = "disabled/inference_surface.rs"]
pub mod inference_surface;
mod inline_pane;
mod inspector_controls;
mod inspector_pane;
mod inspector_view;
#[cfg(feature = "authoring")]
mod knot_authoring;
#[cfg(not(feature = "authoring"))]
#[path = "disabled/knot_authoring.rs"]
mod knot_authoring;
pub mod knot_document_surface;
#[cfg(feature = "authoring")]
mod knot_saved_citations;
pub mod launch;
pub mod nomadnet;
pub mod observe;
mod offline;
pub(crate) mod outer_frame;
mod overmap;
mod pane;
pub mod pane_binding;
/// Turnstone's own pane model (was the `frisket` crate; folded in 2026-07-25).
pub mod panes;
pub mod place;
mod place_service;
pub mod producer_retention;
#[cfg(feature = "peers")]
mod publish_pane;
#[cfg(not(feature = "peers"))]
#[path = "disabled/publish_pane.rs"]
mod publish_pane;
#[cfg(feature = "peers")]
mod publish_service;
#[cfg(not(feature = "peers"))]
#[path = "disabled/publish_service.rs"]
mod publish_service;
mod recycle;
pub mod redshank_episode_surface;
pub mod redshank_host;
pub mod relationship_navigation;
pub mod remote_projection;
mod research;
pub mod resident_admission;
pub mod resident_runs;
#[cfg(test)]
mod resident_runs_tests;
pub mod retained_clip;
mod ring;
mod roster_view;
pub mod saved_sources;
mod scenario;
mod scene_state;
#[cfg(test)]
mod scene_state_tests;
#[cfg(feature = "piccolo")]
mod script;
mod sections;
mod service_init;
#[cfg(feature = "servo")]
#[path = "shell/servo.rs"]
pub mod servo_host;
mod session;
mod session_graph_file;
mod session_persistence;
mod settings_pane;
pub mod settings_provider;
#[cfg(feature = "peers")]
mod share_reader_pane;
#[cfg(not(feature = "peers"))]
#[path = "disabled/share_reader_pane.rs"]
mod share_reader_pane;
#[cfg(feature = "peers")]
mod share_reader_service;
#[cfg(not(feature = "peers"))]
#[path = "disabled/share_reader_service.rs"]
mod share_reader_service;
pub mod shell;
pub mod shell_services;
#[cfg(feature = "sky")]
pub mod sky_receipt;
#[cfg(feature = "sky")]
pub mod sky_surface;
#[cfg(feature = "sky")]
pub mod sky_timeline;
pub mod source_capture;
mod surface;
mod surface_controls;
mod swatch_pane;
mod trail_memory;
mod trail_pane;
mod trail_view;
mod transcript_pane;
mod ui;
mod unavailable_pane;
pub mod user_agent_decision;
pub mod web_policy;
mod window_projection;
mod workbench_pane;
mod workbench_tiling;
