// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Native authorities hosted by Graphshell.

pub mod app_admission;
pub mod app_broker;
#[cfg(test)]
mod app_broker_tests;
pub mod app_client;
pub mod browser_host;
#[cfg(feature = "personal-sync")]
pub mod carriage_host;
pub mod custody;
pub mod custody_client;
pub mod custody_identity;
pub mod device_broker;
pub mod endpoint_catalog;
#[cfg(feature = "personal-sync")]
pub mod graph_keys;
pub mod local_endpoint;
pub mod local_session;
#[cfg(feature = "personal-sync")]
pub mod personal_sync_host;
pub mod projection_host;
pub mod resident_identity;
pub mod tasks;
#[cfg(all(feature = "personal-sync", not(target_arch = "wasm32")))]
pub mod transfer_staging;
