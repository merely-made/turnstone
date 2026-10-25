// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The server over the in-memory notes route the tests use, for trying an MCP
//! client against it without a resident:
//!
//! ```text
//! claude mcp add notes-fixture -- cargo run -q -p graphshell-mcp --example notes_route
//! ```

use std::io::{self, BufReader};

use graphshell_mcp::{Config, Server};

#[path = "../tests/common/mod.rs"]
mod common;

fn main() -> io::Result<()> {
    let config = Config {
        scope: "fixture".into(),
        ..Config::default()
    };
    let route = common::Route::new(&[
        "The fixture route holds notes.",
        "Append more with mere_invoke.",
    ]);
    Server::new(route, config).serve(BufReader::new(io::stdin()), io::stdout())
}
