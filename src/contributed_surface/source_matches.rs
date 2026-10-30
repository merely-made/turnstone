// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn source_matches(expected: &SourceSchemaId, source: &PaneSource) -> bool {
    source_schema(source).is_some_and(|actual| actual == expected)
}
