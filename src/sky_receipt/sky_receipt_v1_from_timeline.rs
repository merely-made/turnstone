// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyReceiptV1 {
    /// Project a live typed timeline into the stable V1 receipt schema.
    pub fn from_timeline(timeline: &SkyTimeline) -> Self {
        let receipt = timeline.receipt();
        assert_eq!(
            receipt.facts().len(),
            timeline.items().len(),
            "the typed timeline and derivation manifest must retain the same facts"
        );
        Self {
            version: SKY_RECEIPT_VERSION,
            engine: SkyEngineV1::turquet(),
            day: SkyDayV1::from_receipt(receipt),
            observer: SkyObserverV1::from_observer(receipt.observer()),
            anchor_tt: TtJulianDateV1::from_tt(receipt.anchor_tt()),
            search: SkySearchV1 {
                sample_step_nanoseconds: duration_nanoseconds(
                    receipt.search().sample_step(),
                    "Sky sample step",
                ),
                tolerance_nanoseconds: duration_nanoseconds(
                    receipt.search().tolerance(),
                    "Sky search tolerance",
                ),
            },
            bodies: receipt
                .bodies()
                .iter()
                .copied()
                .map(SkyBodyPolicyV1::from_selection)
                .collect(),
            ephemeris: SkyEphemerisV1 {
                model: SkyModelV1::from_model(receipt.provider_model()),
                snapshot: receipt.provider_snapshot().map(str::to_owned),
                angular_accuracy: receipt
                    .provider_accuracy()
                    .map(SkyAccuracyV1::from_accuracy),
            },
            observer_transform: SkyObserverTransformV1 {
                model: SkyModelV1::from_model(receipt.observer_transform_model()),
                angular_accuracy: SkyAccuracyV1::from_accuracy(
                    receipt.observer_transform_accuracy(),
                ),
            },
            earth_orientation: SkySourceIdentityV1 {
                authority: receipt.earth_orientation_authority().to_owned(),
                snapshot: receipt.earth_orientation_snapshot().to_owned(),
            },
            facts: receipt
                .facts()
                .iter()
                .copied()
                .zip(timeline.items())
                .map(|(fact, item)| SkyFactV1::from_item(fact, item))
                .collect(),
        }
    }

    /// Serialize using Turnstone's deterministic V1 field and vector order.
    ///
    /// This is the exact byte sequence used by [`Self::blake3_hex_digest`].
    /// It is a Turnstone schema guarantee, not an RFC 8785 claim.
    pub fn to_pretty_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    /// Deserialize and validate a V1 receipt.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SkyReceiptJsonError> {
        let receipt: Self = serde_json::from_slice(bytes).map_err(SkyReceiptJsonError::Json)?;
        receipt.validate()?;
        Ok(receipt)
    }

    /// BLAKE3 digest of the exact bytes returned by [`Self::to_pretty_json`].
    pub fn blake3_hex_digest(&self) -> Result<String, serde_json::Error> {
        let bytes = self.to_pretty_json()?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub(super) fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        if self.version != SKY_RECEIPT_VERSION {
            return Err(SkyReceiptJsonError::UnsupportedVersion(self.version));
        }
        self.engine.validate()?;
        let (day_start, day_end) = self.day.validate()?;
        self.observer.validate()?;
        self.search.validate()?;
        self.anchor_tt.validate("anchor TT")?;
        if self.anchor_tt.summed_day() < day_start.summed_day()
            || self.anchor_tt.summed_day() >= day_end.summed_day()
        {
            return Err(invalid("Sky anchor must belong to the half-open civil day"));
        }

        if self.bodies.is_empty() {
            return Err(invalid("a Sky receipt requires at least one selected body"));
        }
        let mut selected_bodies = HashSet::with_capacity(self.bodies.len());
        for (index, selection) in self.bodies.iter().enumerate() {
            selection.validate(index)?;
            if !selected_bodies.insert(selection.body) {
                return Err(invalid(format!(
                    "Sky body {:?} is selected more than once",
                    selection.body
                )));
            }
        }

        self.ephemeris.validate()?;
        self.observer_transform.validate()?;
        self.earth_orientation
            .validate("Earth-orientation source")?;

        let mut position_bodies = HashSet::with_capacity(selected_bodies.len());
        let mut previous_sort_key: Option<(f64, u8, u8)> = None;
        for (index, fact) in self.facts.iter().enumerate() {
            fact.span_tt.start.validate("fact TT start")?;
            fact.span_tt.end.validate("fact TT end")?;
            if fact.span_tt.start.summed_day() > fact.span_tt.end.summed_day() {
                return Err(invalid(format!(
                    "Sky fact {index} has decreasing TT bounds"
                )));
            }
            if !selected_bodies.contains(&fact.body) {
                return Err(invalid(format!(
                    "Sky fact {index} refers to an unselected body"
                )));
            }

            let midpoint = fact.span_tt.midpoint_day();
            if midpoint < day_start.summed_day() || midpoint >= day_end.summed_day() {
                return Err(invalid(format!(
                    "Sky fact {index} midpoint does not belong to the half-open civil day"
                )));
            }
            let sort_key = (midpoint, fact.kind.sort_rank(), fact.body.sort_rank());
            if previous_sort_key.is_some_and(|previous| {
                previous
                    .0
                    .total_cmp(&sort_key.0)
                    .then_with(|| previous.1.cmp(&sort_key.1))
                    .then_with(|| previous.2.cmp(&sort_key.2))
                    .is_gt()
            }) {
                return Err(invalid(format!(
                    "Sky fact {index} does not follow the production midpoint/kind/body sort order"
                )));
            }
            previous_sort_key = Some(sort_key);

            fact.validate(index)?;
            if fact.kind == SkyFactKindV1::Position {
                if fact.span_tt.start != fact.span_tt.end {
                    return Err(invalid(format!(
                        "Sky position fact {index} must have a zero-width TT span"
                    )));
                }
                if fact.span_tt.start != self.anchor_tt {
                    return Err(invalid(format!(
                        "Sky position fact {index} must be evaluated at the receipt anchor"
                    )));
                }
                if !position_bodies.insert(fact.body) {
                    return Err(invalid(format!(
                        "Sky body {:?} has more than one position fact",
                        fact.body
                    )));
                }
            }
        }
        if position_bodies != selected_bodies {
            return Err(invalid(
                "each selected Sky body must have exactly one position fact",
            ));
        }
        Ok(())
    }
}
