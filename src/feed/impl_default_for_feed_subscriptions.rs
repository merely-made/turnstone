// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for FeedSubscriptions {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            subscriptions: BTreeMap::new(),
            in_flight: BTreeSet::new(),
        }
    }
}
