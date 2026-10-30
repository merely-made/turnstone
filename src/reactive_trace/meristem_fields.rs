// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use tracing::field::{Field, Visit};

use super::{MeristemKind, MeristemReactiveEvent, MeristemReason, ReactiveDiagnosticPayload};

#[derive(Default)]
pub(super) struct MeristemFieldsVisitor {
    schema_version: Option<u16>,
    producer_id: Option<u64>,
    sequence: Option<u64>,
    kind: Option<MeristemKind>,
    generation: Option<u64>,
    generation_present: Option<bool>,
    path_depth: Option<u16>,
    path_depth_present: Option<bool>,
    path_truncated: Option<bool>,
    reason: Option<MeristemReason>,
    last_sequence: Option<u64>,
    invalid: bool,
}

impl MeristemFieldsVisitor {
    pub(super) fn to_payload(&self) -> Option<ReactiveDiagnosticPayload> {
        if self.invalid {
            return None;
        }
        let schema_version = self.schema_version?;
        if schema_version != 1 {
            return None;
        }
        let producer_id = self.producer_id?;
        let kind = self.kind?;
        if let (Some(sequence), Some(path_truncated)) = (self.sequence, self.path_truncated) {
            let generation = match self.generation_present? {
                true => Some(self.generation?),
                false => None,
            };
            let path_depth = match self.path_depth_present? {
                true => Some(self.path_depth?),
                false => None,
            };
            return Some(ReactiveDiagnosticPayload::Meristem(
                MeristemReactiveEvent::Decision {
                    schema_version,
                    producer_id,
                    sequence,
                    kind,
                    generation,
                    path_depth,
                    path_truncated,
                },
            ));
        }
        Some(ReactiveDiagnosticPayload::Meristem(
            MeristemReactiveEvent::CoverageGap {
                schema_version,
                producer_id,
                kind,
                reason: self.reason?,
                last_sequence: self.last_sequence,
            },
        ))
    }
}

impl Visit for MeristemFieldsVisitor {
    fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
        // tracing records a fixed string literal message through `record_debug`.
        // It is ignored here and never enters the typed payload; every other
        // debug-formatted field remains rejected.
        if field.name() != "message" {
            self.invalid = true;
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        match (field.name(), value) {
            ("kind", "any_view_reused") => self.kind = Some(MeristemKind::AnyViewReused),
            ("kind", "any_view_replaced") => self.kind = Some(MeristemKind::AnyViewReplaced),
            ("kind", "any_view_torn_down") => self.kind = Some(MeristemKind::AnyViewTornDown),
            ("kind", "generation_matched") => self.kind = Some(MeristemKind::GenerationMatched),
            ("kind", "generation_refused") => self.kind = Some(MeristemKind::GenerationRefused),
            ("kind", "message_path_advanced") => {
                self.kind = Some(MeristemKind::MessagePathAdvanced)
            },
            ("kind", "message_consumed") => self.kind = Some(MeristemKind::MessageConsumed),
            ("kind", "message_declined") => self.kind = Some(MeristemKind::MessageDeclined),
            ("kind", "sequence_child_added") => self.kind = Some(MeristemKind::SequenceChildAdded),
            ("kind", "sequence_child_removed") => {
                self.kind = Some(MeristemKind::SequenceChildRemoved)
            },
            ("kind", "sequence_message_matched") => {
                self.kind = Some(MeristemKind::SequenceMessageMatched)
            },
            ("kind", "sequence_message_refused") => {
                self.kind = Some(MeristemKind::SequenceMessageRefused)
            },
            ("kind", "coverage_unavailable") => self.kind = Some(MeristemKind::CoverageUnavailable),
            ("reason", "sequence_exhausted") => {
                self.reason = Some(MeristemReason::SequenceExhausted)
            },
            ("reason", "target_lacks_atomic_u64") => {
                self.reason = Some(MeristemReason::TargetLacksAtomicU64)
            },
            ("message", "meristem reactive decision")
            | ("message", "reactive diagnostic coverage gap") => {},
            _ => self.invalid = true,
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "schema_version" => self.schema_version = u16::try_from(value).ok(),
            "producer_id" => self.producer_id = Some(value),
            "sequence" => self.sequence = Some(value),
            "generation" => self.generation = Some(value),
            "path_depth" => self.path_depth = u16::try_from(value).ok(),
            "last_sequence" => self.last_sequence = Some(value),
            _ => self.invalid = true,
        }
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        match field.name() {
            "generation_present" => self.generation_present = Some(value),
            "path_depth_present" => self.path_depth_present = Some(value),
            "path_truncated" => self.path_truncated = Some(value),
            _ => self.invalid = true,
        }
    }
}
