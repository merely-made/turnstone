// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl fmt::Display for SurfaceAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GenerationExhausted => write!(f, "contributed admission generation exhausted"),
            Self::ProviderNotFound {
                pane_kind,
                source_schema,
            } => write!(
                f,
                "no surface provider for pane '{}' and source schema {:?}",
                pane_kind.as_str(),
                source_schema.as_ref().map(SourceSchemaId::as_str)
            ),
            Self::InvalidSource { expected, actual } => write!(
                f,
                "surface source schema {:?} does not match expected {:?}",
                actual.as_ref().map(SourceSchemaId::as_str),
                expected.as_str()
            ),
            Self::InvalidPayload { schema, message } => write!(
                f,
                "surface source payload for {:?} is invalid: {message}",
                schema.as_str()
            ),
            Self::Unavailable { reason } => write!(f, "surface unavailable: {reason:?}"),
        }
    }
}
