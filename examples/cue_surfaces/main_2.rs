// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(target_os = "macos")]
pub(super) fn main() {
    if let Err(e) = native::run() {
        eprintln!("CUE RECEIVING FAILED: {e}");
        std::process::exit(1);
    }
}
