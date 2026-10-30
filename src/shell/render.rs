// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Rendering: each surface's scene, the layered present, and the self-capture
//! that writes a receipt's PNG.
//!
//! Two passes on purpose. The first is mutable (a content session paints its
//! own frame); the second rasterizes and composes immutably, so a session's
//! mutable borrow never overlaps the host's. The capture path composes the
//! same layer list the presented frame did, so a receipt shows what was shown.

use super::{NodeSessions, NodeSurfaces};
use std::path::Path;

use genet_winit_host::SurfaceHost;
use image::ImageEncoder;
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, Scene};

use crate::action::Action;
use crate::frame_timing::ms;
use crate::panes::PaneContent;
use crate::surface::SurfaceKind;

use winit::dpi::{PhysicalPosition, PhysicalSize};

use super::{CompositeLayer, PlannedLayer, PlannedScene, Shell, pane_display_label};

#[path = "render/decode_sprite.rs"]
mod decode_sprite;
#[path = "render/shell_answer_permission_request.rs"]
mod shell_answer_permission_request;
#[path = "render/shell_import_content_layer.rs"]
mod shell_import_content_layer;
#[path = "render/shell_render.rs"]
mod shell_render;
#[path = "render/shell_request_document_work_redraws.rs"]
mod shell_request_document_work_redraws;
pub(super) use decode_sprite::decode_sprite;
#[path = "render/capture_composed.rs"]
mod capture_composed;
pub(super) use capture_composed::capture_composed;
#[path = "render/clear_composition_target.rs"]
mod clear_composition_target;
pub(super) use clear_composition_target::clear_composition_target;
#[path = "render/capture_target.rs"]
mod capture_target;
use capture_target::capture_target;
#[path = "render/start_capture_composed.rs"]
mod start_capture_composed;
use start_capture_composed::start_capture_composed;
#[path = "render/read_texture_rgba.rs"]
mod read_texture_rgba;
pub(super) use read_texture_rgba::read_texture_rgba;
#[path = "render/lens_content_frame.rs"]
mod lens_content_frame;
pub(super) use lens_content_frame::lens_content_frame;
