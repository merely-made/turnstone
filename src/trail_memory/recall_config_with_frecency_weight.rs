// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl RecallConfig {
    /// Weight of the behavioural ranking relative to BM25; zero is the lane's
    /// off switch. The fold's own policy (per-transition weights, half-life,
    /// dwell bonus) is eidetic's, not the app's. No live setting moves it now
    /// that the vector knobs are gone, so only the harnesses call it.
    #[allow(dead_code)]
    pub fn with_frecency_weight(mut self, weight: f32) -> Self {
        self.frecency_weight = Self::clamp_weight(weight);
        self
    }

    pub(super) fn clamp_weight(weight: f32) -> f32 {
        if weight.is_finite() {
            weight.clamp(0.0, 4.0)
        } else {
            0.0
        }
    }

    pub(super) fn frecency_enabled(self) -> bool {
        self.frecency_weight > 0.0
    }
}
