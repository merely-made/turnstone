// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use std::process::ExitCode;
use tabard_desktop::{Command, USAGE, parse_arguments, run};

fn main() -> ExitCode {
    let result = parse_arguments(std::env::args_os().skip(1))
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
        .and_then(|command| match command {
            Command::Help => {
                println!("{USAGE}");
                Ok(())
            },
            Command::Run { library } => run(library),
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tabard: {error}");
            ExitCode::FAILURE
        },
    }
}
