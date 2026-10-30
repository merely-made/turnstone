// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Drop for InterceptionPage {
    fn drop(&mut self) {
        self.lifetime.lock().unwrap().push("drop".into());
    }
}
