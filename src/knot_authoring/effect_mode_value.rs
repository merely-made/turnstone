// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The already-validated mode string as Knot's own enum.
///
/// `effect_mode` rejected anything else on the way in, so an unknown value
/// here would be a bug upstream rather than user input; the safe reading is
/// the one that grants nothing.
pub(super) fn effect_mode_value(mode: &str) -> knot::KnotEffectMode {
    match mode {
        "auto" => knot::KnotEffectMode::Auto,
        "ask" => knot::KnotEffectMode::Ask,
        _ => knot::KnotEffectMode::Never,
    }
}
