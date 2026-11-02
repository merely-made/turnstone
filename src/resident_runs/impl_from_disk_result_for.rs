// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<DiskResult> for ResultKind {
    fn from(v: DiskResult) -> Self {
        match v {
            DiskResult::Progress => Self::Progress,
            DiskResult::Completed => Self::Completed,
            DiskResult::Refused => Self::Refused,
            DiskResult::Failed => Self::Failed,
            DiskResult::RetryableFailure => Self::RetryableFailure,
        }
    }
}
