// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone's window and owner service for the shared Alembic surface.

use super::Shell;
use alembic::workshop::{Control, Destination, Owner, Review};
use alembic::workshop_native::{NativeHost, native_hooks, native_init};
use alembic::workshop_surface::{RetainedReading, SourceChoice, SurfaceAction, SurfaceState};
use athanor::workshop_driver::Driver;
use cambium_genet_winit_host::{CloseRequest, HostOptions, WindowFrame, WinitHost};
use cambium_rootstock::IdlePolicy;
use muniment::DirectoryBackend;
use std::sync::Arc;
use winit::{event::WindowEvent, event_loop::ActiveEventLoop, window::WindowId};

pub(super) struct WorkshopWindow {
    pub host: NativeHost,
    driver: Option<Driver<DirectoryBackend>>,
    owner: Owner,
    installation: [u8; 16],
    human: [u8; 32],
    subject: [u8; 32],
}

#[cfg(test)]
#[path = "alembic_workshop/reconcile_completion_tests.rs"]
mod reconcile_completion_tests;

#[path = "alembic_workshop/workshop_passage_current.rs"]
mod workshop_passage_current;
use workshop_passage_current::workshop_passage_current;
#[path = "alembic_workshop/install_destination_then_acknowledge.rs"]
mod install_destination_then_acknowledge;
use install_destination_then_acknowledge::install_destination_then_acknowledge;
#[path = "alembic_workshop/subject.rs"]
mod subject;
use subject::subject;
#[path = "alembic_workshop/installation.rs"]
mod installation;
use installation::installation;
#[path = "alembic_workshop/shell_accept_or_reconcile_alembic.rs"]
mod shell_accept_or_reconcile_alembic;
#[path = "alembic_workshop/shell_handle_alembic_action.rs"]
mod shell_handle_alembic_action;
#[path = "alembic_workshop/shell_service_alembic_workshop.rs"]
mod shell_service_alembic_workshop;
#[path = "alembic_workshop/workshop_window_state.rs"]
mod workshop_window_state;
