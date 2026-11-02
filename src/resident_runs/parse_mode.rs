// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn parse_mode(s: &str) -> Result<Mode, String> {
    match s {
        "Read" => Ok(Mode::Read),
        "Write" => Ok(Mode::Write),
        "Delegate" => Ok(Mode::Delegate),
        _ => Err("invalid stored run mode".into()),
    }
}
