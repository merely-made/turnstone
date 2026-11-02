// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::ffi::OsString;
use std::path::PathBuf;
#[path = "knot_authoring/is_knot_address.rs"]
mod is_knot_address;
pub(crate) use is_knot_address::is_knot_address;
#[path = "knot_authoring/place_held_address.rs"]
mod place_held_address;
pub(crate) use place_held_address::place_held_address;
#[path = "knot_authoring/parse_place_held_address.rs"]
mod parse_place_held_address;
pub(crate) use parse_place_held_address::parse_place_held_address;
#[path = "knot_authoring/place_held_rewrite.rs"]
mod place_held_rewrite;
pub(crate) use place_held_rewrite::place_held_rewrite;
#[path = "knot_authoring/vault_relative.rs"]
mod vault_relative;
use vault_relative::vault_relative;
#[path = "knot_authoring/normalized_absolute.rs"]
mod normalized_absolute;
use normalized_absolute::normalized_absolute;
#[path = "knot_authoring/env_path.rs"]
mod env_path;
pub(crate) use env_path::env_path;
