// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn saved_target(key: &Key, text_hash: &str) -> Target {
    Target {
        key: key.clone(),
        text_hash: text_hash.to_owned(),
        selector: None,
    }
}
