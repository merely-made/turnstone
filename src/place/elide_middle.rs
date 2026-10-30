// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// `value` with its middle replaced by an ellipsis, or `value` unchanged when
/// eliding would not shorten it. Char-based, so a multi-byte value is never
/// cut mid-character.
pub fn elide_middle(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= ELIDE_HEAD + ELIDE_TAIL + 1 {
        return value.to_string();
    }
    let head: String = chars[..ELIDE_HEAD].iter().collect();
    let tail: String = chars[chars.len() - ELIDE_TAIL..].iter().collect();
    format!("{head}…{tail}")
}
