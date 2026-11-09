// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The Dramatis pane: a Graphshell host's view of Dramatis's projection
//! surface.
//!
//! Dramatis is a producer, not a window. It serves its roster, quarantine
//! review, vault lock and passphrase change as portable cards plus bounded
//! action forms ([`dramatis_surface`]); this crate is the host side that
//! resolves those cards, draws them as retained, keyboard-reachable controls,
//! and sends the chosen intent back. It draws nothing the surface did not
//! advertise, takes no text input, and holds no secret: Djinn still shows the
//! consent and passphrase prompts natively.
//!
//! The surface runs in the host's process here. Running it as a stdio child
//! waits for that carrier to authenticate its peer.
mod pane;
mod view;

pub use pane::{Pane, PaneCard};
pub use view::{Access, Child, SHEET, follow_focus_hooks, view};
