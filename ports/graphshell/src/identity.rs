// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The secret-free identity read model Graphshell projects.
//!
//! The types live in `dramatis::view` (dramatis repo plan, DR-A). Since DR-C
//! Graphshell reaches them there directly rather than through castellan,
//! which only djinn links (rulings D4, D15).

pub use dramatis::view::*;
