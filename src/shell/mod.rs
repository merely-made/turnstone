// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The desktop shell: winit window + the shared present stack, raw input
//! mapped onto the canvas's semantic methods (continuous gestures) and onto
//! [`Action`]s (app intents), the ports (fetch + physics actors), and the
//! effect runner. The only module that touches a platform API; everything it
//! learns flows back through the spine.

#[cfg(feature = "workshop")]
mod alembic_workshop;
#[cfg(not(feature = "workshop"))]
#[path = "../disabled/alembic_workshop.rs"]
mod alembic_workshop;
mod appearance_editor;
mod appearance_receipt;
mod clip_actions;
mod clip_reading;
mod clip_retaining;
mod content_pool;
mod contributed_automation;
mod controller_input;
mod cue_content;
mod document_a11y;
mod citation_event_history;
mod drive;
mod effects;
mod events;
mod foreign_a11y;
mod gestures;
mod keys;
mod native_frames;
mod nomadnet_fetch;
mod page_capture;
mod reader_observe;
mod render;
mod renderers;
mod research_notes;
mod retained_clips;
mod saved_sources;
#[cfg(all(feature = "scry", windows))]
mod scry;
#[cfg(all(
    feature = "servo",
    any(windows, target_os = "macos", target_os = "linux")
))]
use crate::servo_host as servo;
#[cfg(all(any(feature = "weld", feature = "scry", feature = "servo"), windows))]
mod surface_frames;
mod surface_poll;
#[cfg(all(feature = "weld", windows))]
pub(crate) mod weld;
use render::{capture_composed, decode_sprite};
mod lens;
use lens::LensWindow;
mod frame_keys;
mod input;
mod native_composition;
mod outer_workspace;
mod titlebar;
pub(crate) mod window_input;
use input::pointer_button;

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use fetch::{FetchCommand, FetchUpdate};
use genet_documents::LocalFetcher;
use genet_winit_host::SurfaceHost;
use image::ImageEncoder;
use inker::{DocumentSession, SessionClick, SessionRegistry, SessionSpawnRequest};
use mere::canvas::WHEEL_PAN_SCALE;
use mere_document_lanes::{
    ReaderSessionEngine, RemoteFetcher, SmolwebInlineMediaPolicy, SmolwebSessionEngine,
    SmolwebTheme,
};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};

pub(crate) mod a11y_bridge;
mod window_a11y;

use crate::panes::PaneContent;
use crate::settings_pane::LiveSettingsHandle;
use crate::settings_provider::ApplicationSettingsProvider;

use crate::action::{Action, Effect, Update};
use crate::app::App;
use crate::surface::{Rect, SurfaceKind};
use crate::{browse, session};

use netrender::Scene;

/// Each node's live content: Pelt's routed content, either a document lane
/// around a host-loading, host-history controller or a surface lane around the
/// web engine's producer (SC step 4). The App owns the node's load and graph
/// history; a document controller opens the body the App fetched, and a
/// surface takes navigation on its own web plane.
pub(crate) use content_pool::NodeDocuments;

#[cfg(test)]
#[path = "servo_profile_tests.rs"]
mod servo_profile_tests;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Native window metadata captured from the shell's current primary/lens records.
/// The portable controls use actual App lens identities and the same host planners.
struct ContributedActionWindows {
    primary: (u32, u32),
    lenses: Vec<(usize, u32, u32)>,
}

#[cfg(test)]
#[path = "contributed_window_a11y_tests.rs"]
mod contributed_window_a11y_tests;

#[cfg(test)]
#[path = "render_recovery_tests.rs"]
mod render_recovery_tests;

#[path = "impl_node_sessions_for_node_documents.rs"]
mod impl_node_sessions_for_node_documents;
#[path = "impl_node_surfaces_for_node_documents.rs"]
mod impl_node_surfaces_for_node_documents;
#[path = "impl_pelt_core_pelt_clock_for.rs"]
mod impl_pelt_core_pelt_clock_for;
#[path = "standard_content_engines.rs"]
mod standard_content_engines;
pub(crate) use standard_content_engines::standard_content_engines;
#[path = "content_engines_with_client_limits.rs"]
mod content_engines_with_client_limits;
use content_engines_with_client_limits::content_engines_with_client_limits;
#[path = "content_engines_with_remote.rs"]
mod content_engines_with_remote;
use content_engines_with_remote::content_engines_with_remote;
#[path = "project_engine_inventory.rs"]
mod project_engine_inventory;
pub(crate) use project_engine_inventory::project_engine_inventory;
#[path = "servo_construction_availability.rs"]
mod servo_construction_availability;
use servo_construction_availability::servo_construction_availability;
#[path = "weld_construction_availability.rs"]
mod weld_construction_availability;
use weld_construction_availability::weld_construction_availability;
#[path = "scry_construction_availability.rs"]
mod scry_construction_availability;
use scry_construction_availability::scry_construction_availability;
#[path = "smolweb_inline_media_policy.rs"]
mod smolweb_inline_media_policy;
use smolweb_inline_media_policy::smolweb_inline_media_policy;
#[path = "standard_route_policy.rs"]
mod standard_route_policy;
use standard_route_policy::standard_route_policy;
#[path = "content_link_target.rs"]
mod content_link_target;
use content_link_target::content_link_target;
#[path = "pane_display_label.rs"]
mod pane_display_label;
use pane_display_label::pane_display_label;
#[path = "start_share_reader.rs"]
mod start_share_reader;
use start_share_reader::start_share_reader;
#[path = "start_publishing.rs"]
mod start_publishing;
use start_publishing::start_publishing;
#[path = "shared_scenario_from_env.rs"]
mod shared_scenario_from_env;
#[path = "shell_start_private_connect.rs"]
mod shell_start_private_connect;
use shared_scenario_from_env::shared_scenario_from_env;
#[path = "servo_profile_directory.rs"]
#[cfg(any(
    test,
    all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    )
))]
mod servo_profile_directory;
#[cfg(any(
    test,
    all(
        feature = "servo",
        any(windows, target_os = "macos", target_os = "linux")
    )
))]
use servo_profile_directory::servo_profile_directory;
#[path = "surface_poll_interval_from_env.rs"]
mod surface_poll_interval_from_env;
use surface_poll_interval_from_env::surface_poll_interval_from_env;
#[path = "shared_out_dir_from_env.rs"]
mod shared_out_dir_from_env;
use shared_out_dir_from_env::shared_out_dir_from_env;
#[path = "frame_placement.rs"]
mod frame_placement;
use frame_placement::frame_placement;
#[path = "take_window_capture.rs"]
mod take_window_capture;
#[path = "window_recovery_new.rs"]
mod window_recovery_new;
use take_window_capture::take_window_capture;
#[path = "raster_capture_refusal.rs"]
mod raster_capture_refusal;
use raster_capture_refusal::raster_capture_refusal;
#[path = "admit_recovered_core.rs"]
mod admit_recovered_core;
use admit_recovered_core::admit_recovered_core;
#[path = "impl_drop_for_shell.rs"]
mod impl_drop_for_shell;
#[path = "primary_content_plan.rs"]
mod primary_content_plan;
#[path = "shell_ensure_render_recovery.rs"]
mod shell_ensure_render_recovery;
#[path = "shell_new.rs"]
mod shell_new;
#[path = "shell_publish_engine_inventory.rs"]
mod shell_publish_engine_inventory;
#[path = "shell_push_primary_a11y_tree.rs"]
mod shell_push_primary_a11y_tree;
#[path = "shell_scenario_pump.rs"]
mod shell_scenario_pump;
#[path = "shell_surface_plan.rs"]
mod shell_surface_plan;
use primary_content_plan::primary_content_plan;
#[path = "primary_content_plan_with_rects.rs"]
mod primary_content_plan_with_rects;
use primary_content_plan_with_rects::primary_content_plan_with_rects;
#[path = "contributed_action_plan.rs"]
mod contributed_action_plan;
use contributed_action_plan::contributed_action_plan;
#[path = "dispatch_contributed_action.rs"]
mod dispatch_contributed_action;
use dispatch_contributed_action::dispatch_contributed_action;
#[path = "smolweb_session_engine_ids.rs"]
mod smolweb_session_engine_ids;
use smolweb_session_engine_ids::SMOLWEB_SESSION_ENGINE_IDS;
#[path = "node_sessions.rs"]
mod node_sessions;
pub(crate) use node_sessions::NodeSessions;
#[path = "node_surfaces.rs"]
mod node_surfaces;
pub(crate) use node_surfaces::NodeSurfaces;
#[path = "shell_clock.rs"]
mod shell_clock;
pub(crate) use shell_clock::ShellClock;
#[path = "pending_trail_cap.rs"]
mod pending_trail_cap;
use pending_trail_cap::PENDING_TRAIL_CAP;
#[path = "private_binding.rs"]
mod private_binding;
use private_binding::PrivateBinding;
#[path = "planned_scene.rs"]
mod planned_scene;
use planned_scene::PlannedScene;
#[path = "planned_layer.rs"]
mod planned_layer;
use planned_layer::PlannedLayer;
#[path = "composite_layer.rs"]
mod composite_layer;
use composite_layer::CompositeLayer;
#[path = "window_recovery.rs"]
mod window_recovery;
use window_recovery::WindowRecovery;
#[path = "host_file_drag.rs"]
mod host_file_drag;
use host_file_drag::HostFileDrag;
#[path = "shell.rs"]
mod shell;
pub use shell::Shell;

/// Wake classification keeps model invalidation apart from GPU completion.
#[derive(Clone, Copy, Debug)]
pub enum ShellEvent {
    ActorWake,
    NativeFrameReady,
}
