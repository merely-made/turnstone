// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Castellan's resident authority, at its pre-founding path.
//!
//! [`PersonaeHost`] and its receipts moved home to [`castellan::authority`]
//! with the keeper founding (2026-08-14). Graphshell composes the keeper — it
//! serves the projection, supplies the desktop dialogs, and binds the agent
//! endpoints — and owns none of it.
//!
//! Since the dramatis repo plan's DR-A (2026-10-09) the receipts live in
//! `dramatis::receipts`; castellan re-exports them beside `PersonaeHost`.

pub use castellan::authority::*;
