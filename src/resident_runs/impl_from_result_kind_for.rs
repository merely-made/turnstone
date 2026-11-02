// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<ResultKind> for DiskResult {
    fn from(v: ResultKind) -> Self {
        match v {
            ResultKind::Progress => Self::Progress,
            ResultKind::Completed => Self::Completed,
            ResultKind::Refused => Self::Refused,
            ResultKind::Failed => Self::Failed,
            ResultKind::RetryableFailure => Self::RetryableFailure,
        }
    }
}
