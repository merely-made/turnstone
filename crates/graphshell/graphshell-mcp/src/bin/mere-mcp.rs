// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! `mere-mcp`: a stdio MCP server over one route of djinn's application door.
//!
//! ```text
//! claude mcp add mere -- mere-mcp --app mcp.claude-code --route mere/main
//! ```
//!
//! The server holds no key. It names itself with `--app`; djinn admits that id
//! only for the routes its owner granted, and attributes accepted edits to the
//! persona via that app. Standard output carries only MCP messages.

use std::io::{self, BufReader};
use std::process::ExitCode;

use djinn::door::app_admission::{AppId, AppRouteId, configured_app_endpoint};
use graphshell_endpoint::door::projection::AppRouteCarrier;
use graphshell_mcp::{Config, Server};

const USAGE: &str = "usage: mere-mcp --route <route> [--app <id>] [--endpoint <endpoint>] [--read-only]\n\
  --route     the route to serve, such as mere/main (required)\n\
  --app       the app id to be admitted as (default mcp.agent)\n\
  --endpoint  djinn's application endpoint (default: GRAPHSHELL_APP_ENDPOINT or the installed one)\n\
  --read-only offer no write tool";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("mere-mcp: {message}");
            ExitCode::FAILURE
        },
    }
}

fn run() -> Result<(), String> {
    let (mut route, mut app, mut endpoint, mut writes) = (
        None,
        "mcp.agent".to_string(),
        configured_app_endpoint(),
        true,
    );
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value\n{USAGE}"));
        match flag.as_str() {
            "--route" => route = Some(value("--route")?),
            "--app" => app = value("--app")?,
            "--endpoint" => endpoint = value("--endpoint")?,
            "--read-only" => writes = false,
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            },
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let route = route.ok_or(format!("--route is required\n{USAGE}"))?;
    let route_id = AppRouteId::new(route.clone()).map_err(|error| error.to_string())?;
    let carrier = AppRouteCarrier::open_at(&endpoint, AppId::new(&app), route_id)
        .map_err(|error| format!("the resident refused {app} on {route}: {error}"))?;
    let config = Config {
        scope: route,
        writes,
        ..Config::default()
    };
    Server::new(carrier, config)
        .serve(BufReader::new(io::stdin()), io::stdout())
        .map_err(|error| error.to_string())
}
