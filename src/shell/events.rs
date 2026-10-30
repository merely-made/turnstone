// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The winit handler: platform events in, delivered input and redraws out.
//!
//! Thin on purpose. Each arm maps a platform event onto the shared delivery
//! seams (`on_key`, `deliver_press`, `deliver_wheel`) that the scenario runner
//! also drives, so one description runs through two runners.

use super::{NodeSessions, NodeSurfaces};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use std::sync::Arc;

use fetch::FetchUpdate;
use genet_winit_host::SurfaceHost;
use mere::canvas::WHEEL_PAN_SCALE;
use netrender::NetrenderOptions;
use winit::dpi::PhysicalSize;

use crate::action::Action;
use crate::browse;

use super::Shell;

#[path = "events/impl_application_handler_for_shell.rs"]
mod impl_application_handler_for_shell;
#[path = "events/shell_drain_actor_updates.rs"]
mod shell_drain_actor_updates;
