// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl pelt_core::PeltClock for Clock {
    fn now_ms(&self) -> f64 {
        self.0.elapsed().as_secs_f64() * 1000.
    }
}
