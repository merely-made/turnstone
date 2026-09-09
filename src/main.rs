// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone: a graph-workspace browser and the reference host for the mere
//! library.
//!
//! Architecture (design_docs/2026-07-10_turnstone_architecture_plan.md): one
//! typed vocabulary. Platform events lower to [`turnstone::action::Action`]s;
//! [`turnstone::app::App::update`] mutates state and returns
//! [`turnstone::action::Effect`]s; [`turnstone::shell::Shell`] runs effects through
//! ports (the fetch and physics actors, the persistence store) and folds their
//! typed answers back through [`turnstone::app::App::apply_update`]. Continuous
//! canvas gestures map onto
//! `mere::canvas`'s semantic input methods directly — the canvas is hosted,
//! not wrapped.
//!
//! Run with an address to open it (the graph remembers across launches), or
//! bare to restore the last session:
//!
//! ```text
//! cargo run -- https://example.com
//! ```
//!
//! Navigation (per the graph-canvas defaults): wheel = pan, Ctrl+wheel =
//! cursor-anchored zoom, middle-drag = pan, all with inertia. Left-drag grabs
//! and pins the node under the cursor; a click selects; a drag on empty space
//! marquee-selects; a bare empty click clears. Space re-seeds the layout;
//! `i` toggles the isometric view, `q`/`e` orbit, `[`/`]` tilt, `h` toggles
//! height-by-degree.

fn main() {
    // The direct, unsandboxed route (see `turnstone::launch` for the other,
    // sandboxed one: CEF's Windows bootstrap calling the sibling
    // `sandbox_bootstrap_win` crate's `RunWinMain` cdylib export).
    std::process::exit(turnstone::launch::run_direct(std::env::args().nth(1)));
}
