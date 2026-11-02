// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn source_schema(source: &PaneSource) -> Option<&SourceSchemaId> {
    match source {
        PaneSource::Fixed(SourceRef::External { schema, .. }) => Some(schema),
        _ => None,
    }
}
