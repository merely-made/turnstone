// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native authorities hosted by Graphshell.
//!
//! The application side of the first-party door (admission, the app client,
//! custody and its clients, quarantine, the local endpoint and tracked tasks)
//! is `graphshell_endpoint::door` (Graphshell stack lift plan, L1). What
//! remains here serves it.

pub mod app_broker;
#[cfg(test)]
mod app_broker_tests;
pub mod browser_host;
#[cfg(feature = "personal-sync")]
pub mod carriage_host;
pub mod device_broker;
pub mod endpoint_catalog;
#[cfg(feature = "personal-sync")]
pub mod graph_keys;
pub mod local_session;
#[cfg(feature = "personal-sync")]
pub mod personal_sync_host;
pub mod projection_host;
pub mod resident_identity;
pub mod resident_mere_proxy;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod transfer_staging;

/// Transitional, and the only re-export of the lifted door: Djinn's pinned
/// `knot-editor` (`8ed49d4`, whose `src/startup.rs` names these three paths)
/// resolves `graphshell` to this package through the workspace patch.
/// Nothing in this repository uses them. Delete this line when Djinn's Knot
/// pin imports `graphshell_endpoint::door` (stack lift plan, P1 Progress).
#[doc(hidden)]
pub use graphshell_endpoint::door::{app_admission, custody, custody_client};
