// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(not(target_os = "macos"))]
pub(super) fn main() {
    eprintln!("The concrete native receiver currently requires macOS/Metal");
    std::process::exit(2);
}
