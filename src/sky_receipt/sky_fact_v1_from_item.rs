// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyFactV1 {
    pub(super) fn from_item(fact: SkyReceiptFact, item: &SkyTimelineItem) -> Self {
        Self {
            body: SkyBodyV1::from_body(fact.body()),
            kind: SkyFactKindV1::from_kind(fact.kind()),
            span_tt: SkyFactSpanV1::from_span(fact.span()),
            calculation_model: fact.calculation_model().map(SkyModelV1::from_model),
            detail: SkyFactDetailV1::from_item(item),
        }
    }

    pub(super) fn validate(&self, index: usize) -> Result<(), SkyReceiptJsonError> {
        let expected_detail = match self.kind {
            SkyFactKindV1::Position => SkyFactDetailKind::Position,
            SkyFactKindV1::NewMoon
            | SkyFactKindV1::FirstQuarter
            | SkyFactKindV1::FullMoon
            | SkyFactKindV1::LastQuarter => SkyFactDetailKind::LunarPhase,
            SkyFactKindV1::Rise | SkyFactKindV1::Set => SkyFactDetailKind::RiseSet,
            SkyFactKindV1::UpperTransit | SkyFactKindV1::LowerTransit => {
                SkyFactDetailKind::MeridianTransit
            },
        };
        if self.detail.kind() != expected_detail {
            return Err(invalid(format!(
                "Sky fact {index} kind and detail projection do not match"
            )));
        }
        if matches!(
            self.kind,
            SkyFactKindV1::NewMoon
                | SkyFactKindV1::FirstQuarter
                | SkyFactKindV1::FullMoon
                | SkyFactKindV1::LastQuarter
        ) && self.body != SkyBodyV1::Moon
        {
            return Err(invalid(format!(
                "Sky lunar-phase fact {index} must name the Moon"
            )));
        }

        let model_required = matches!(
            self.kind,
            SkyFactKindV1::Rise
                | SkyFactKindV1::Set
                | SkyFactKindV1::UpperTransit
                | SkyFactKindV1::LowerTransit
        );
        match (&self.calculation_model, model_required) {
            (Some(model), true) => {
                model.validate(&format!("Sky fact {index} calculation model"))?
            },
            (None, false) => {},
            (Some(_), false) => {
                return Err(invalid(format!(
                    "Sky fact {index} must use the deduplicated receipt-header models"
                )));
            },
            (None, true) => {
                return Err(invalid(format!(
                    "Sky fact {index} is missing its event-family calculation model"
                )));
            },
        }
        self.detail.validate(index)
    }
}
