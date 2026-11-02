// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::fmt::Debug for DecisionCard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Permission {
                request,
                prompt,
                queued,
                submitting,
                error,
            } => formatter
                .debug_struct("Permission")
                .field("request", request)
                .field("prompt", prompt)
                .field("queued", queued)
                .field("submitting", submitting)
                .field("error", error)
                .finish(),
            Self::Authentication {
                request,
                prompt,
                queued,
                field,
                remember_for_process,
                submitting,
                error,
                ..
            } => formatter
                .debug_struct("Authentication")
                .field("request", request)
                .field("prompt", prompt)
                .field("queued", queued)
                .field("username", &"[redacted]")
                .field("password", &"[redacted]")
                .field("field", field)
                .field("remember_for_process", remember_for_process)
                .field("submitting", submitting)
                .field("error", error)
                .finish(),
        }
    }
}
