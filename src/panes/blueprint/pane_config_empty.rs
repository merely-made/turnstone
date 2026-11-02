// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PaneConfig {
    pub fn empty(schema: impl Into<String>) -> Self {
        Self {
            schema: PaneConfigSchemaId::new(schema),
            version: 1,
            payload: Value::Null,
        }
    }
}
