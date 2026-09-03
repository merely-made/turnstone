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
pub mod app;
mod apparatus_pane;
pub mod behaviors;
mod browse;
mod cambium_pane;
mod chrome_view;
#[cfg(feature = "wasm")]
mod component;
mod content;
mod content_classes;
mod contributed_a11y;
pub mod contributed_surface;
mod denizen;
mod device_receipts_pane;
mod device_receipts_service;
pub mod distillery_installed_surface;
pub mod document_find;
mod download;
mod feed;
pub mod frame_timing;
mod frozen_projection_pane;
mod gemini_identity;
mod gemini_trust;
mod identity;
mod inspector_pane;
mod inspector_view;
mod knot_authoring;
pub mod knot_document_surface;
pub mod observe;
mod overmap;
mod pane;
/// Turnstone's own pane model (was the `frisket` crate; folded in 2026-07-25).
pub mod panes;
pub mod place;
mod publish_pane;
mod publish_service;
mod recycle;
pub mod remote_projection;
mod ring;
mod roster_view;
mod scenario;
#[cfg(feature = "piccolo")]
mod script;
mod sections;
mod session;
mod settings_pane;
pub mod settings_provider;
mod share_reader_pane;
mod share_reader_service;
pub mod shell;
pub mod shell_services;
pub mod sky_receipt;
pub mod sky_surface;
pub mod sky_timeline;
mod steward_pane;
mod surface;
mod swatch_pane;
mod trail_memory;
mod trail_pane;
mod trail_view;
mod transcript_pane;
mod ui;
pub mod user_agent_decision;
pub mod web_policy;
mod workbench_pane;
mod workbench_tiling;
