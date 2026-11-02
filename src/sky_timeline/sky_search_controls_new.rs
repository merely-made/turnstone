// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkySearchControls {
    pub fn new(sample_step: Duration, tolerance: Duration) -> Result<Self, SkySearchControlsError> {
        if sample_step.is_zero() {
            return Err(SkySearchControlsError::SampleStepZero);
        }
        if sample_step > MAX_SAMPLE_STEP {
            return Err(SkySearchControlsError::SampleStepTooLarge);
        }
        if tolerance.is_zero() {
            return Err(SkySearchControlsError::ToleranceZero);
        }
        if tolerance > sample_step {
            return Err(SkySearchControlsError::ToleranceExceedsStep);
        }
        Ok(Self {
            sample_step,
            tolerance,
        })
    }

    pub fn one_hour_one_second() -> Self {
        Self {
            sample_step: MAX_SAMPLE_STEP,
            tolerance: Duration::from_secs(1),
        }
    }

    pub fn sample_step(self) -> Duration {
        self.sample_step
    }

    pub fn tolerance(self) -> Duration {
        self.tolerance
    }

    pub(super) fn step_days(self) -> f64 {
        self.sample_step.as_secs_f64() / 86_400.0
    }

    pub(super) fn tolerance_days(self) -> f64 {
        self.tolerance.as_secs_f64() / 86_400.0
    }
}
