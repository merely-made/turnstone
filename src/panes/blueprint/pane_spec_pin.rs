// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PaneSpec {
    pub fn pin(&mut self, source: SourceRef) {
        self.source = PaneSource::Fixed(source);
        self.context = ContextBinding::Own;
    }
}
