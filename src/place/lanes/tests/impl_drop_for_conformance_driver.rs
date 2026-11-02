// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for ConformanceDriver {
    fn drop(&mut self) {
        self.release_member();
        release(&self.founder.0);
    }
}
