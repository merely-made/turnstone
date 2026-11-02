// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl TurnstoneScryEngine {
    pub(in super::super) fn new(factory: Arc<TurnstoneScryFactory>) -> Self {
        Self { factory }
    }
}
