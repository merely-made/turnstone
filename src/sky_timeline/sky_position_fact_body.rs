// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyPositionFact {
    pub fn body(&self) -> ApparentBody {
        self.body
    }

    pub fn epoch(&self) -> JulianDate<TerrestrialTime> {
        self.epoch
    }

    pub fn observation(&self) -> &Modelled<Observation> {
        &self.observation
    }
}
