// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Host controls for session placement, refusal, and publication ordering.
use super::App;
use crate::action::{Action, Effect};
use crate::session;
use crate::session_persistence::SessionPersistence;
use mere::kernel::graph::{Graph, apply::add_node};
use pandect::graph_placement::PlacementProfile;

#[path = "session_persistence_tests/host_session_refusal_blocks_save_fork_background_and_cases.rs"]
mod host_session_refusal_blocks_save_fork_background_and_cases;
#[path = "session_persistence_tests/refused_session_lease_clears_the_departed_sessions_live_cases.rs"]
mod refused_session_lease_clears_the_departed_sessions_live_cases;
