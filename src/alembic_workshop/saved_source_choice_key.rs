// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn saved_source_choice_key(entry: &Entry) -> Option<String> {
    let content = entry.content.as_ref()?;
    saved_sources::address(&saved_target(&entry.admission.key, &content.text_hash)).ok()
}
