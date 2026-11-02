// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for LivenessWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
