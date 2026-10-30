// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyDerivationReceipt {
    pub fn day(&self) -> &ResolvedSkyDay {
        &self.day
    }

    pub fn observer(&self) -> Observer {
        self.observer
    }

    pub fn anchor_tt(&self) -> JulianDate<TerrestrialTime> {
        self.anchor_tt
    }

    pub fn search(&self) -> SkySearchControls {
        self.search
    }

    pub fn bodies(&self) -> &[SkyBodySelection] {
        &self.bodies
    }

    pub fn provider_model(&self) -> Model {
        self.provider_model
    }

    pub fn provider_accuracy(&self) -> Option<Accuracy> {
        self.provider_accuracy
    }

    pub fn provider_snapshot(&self) -> Option<&str> {
        self.provider_snapshot.as_deref()
    }

    pub fn observer_transform_model(&self) -> Model {
        self.observer_transform_model
    }

    pub fn observer_transform_accuracy(&self) -> Accuracy {
        self.observer_transform_accuracy
    }

    pub fn earth_orientation_authority(&self) -> &str {
        &self.earth_orientation_authority
    }

    pub fn earth_orientation_snapshot(&self) -> &str {
        &self.earth_orientation_snapshot
    }

    pub fn facts(&self) -> &[SkyReceiptFact] {
        &self.facts
    }
}
