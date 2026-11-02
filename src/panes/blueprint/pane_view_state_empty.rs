// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PaneViewState {
    pub fn empty(schema: impl Into<String>) -> Self {
        Self {
            schema: PaneViewSchemaId::new(schema),
            version: 1,
            payload: Value::Null,
        }
    }
}
