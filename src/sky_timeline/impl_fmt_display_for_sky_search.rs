// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl fmt::Display for SkySearchControlsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SampleStepZero => "Sky sample step must be positive",
            Self::SampleStepTooLarge => "Sky sample step cannot exceed one hour",
            Self::ToleranceZero => "Sky search tolerance must be positive",
            Self::ToleranceExceedsStep => "Sky search tolerance cannot exceed the sample step",
        })
    }
}
