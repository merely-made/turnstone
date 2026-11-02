// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Enforce L1 at the storage boundary as well as at browse enrichment: any
/// queued or older caller of `RecordText` is harmless. Existing PageText
/// records remain untouched and readable through the legacy Answers adapter.
pub(super) fn keep_page_text(_store: &FjallStore, url: &str, _text: &str) {
    tracing::debug!(%url, "trail memory: ordinary page text withheld under L1");
}
