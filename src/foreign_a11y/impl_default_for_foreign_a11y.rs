// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for ForeignA11y {
    fn default() -> Self {
        Self {
            surfaces: HashMap::new(),
            owners: GraftTable::new(TreeId::ROOT),
            publication: Grafts::new(),
            mounted: HashSet::new(),
            dirty: false,
            published: HashMap::new(),
            published_forest: Vec::new(),
        }
    }
}
