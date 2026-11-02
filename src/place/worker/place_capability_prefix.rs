// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// `cap_path` encodes a scope as `scope/<path>`, and personae matches on a
/// slash boundary, so this one prefix covers both `commons/container/...`
/// and `commons/chat/...`. Deriving it beats writing the literal: the
/// encoding is servitor's to change.
pub fn place_capability_prefix() -> String {
    cap_path(&Cap::scope("commons").expect("the `commons` scope is well formed"))
}
