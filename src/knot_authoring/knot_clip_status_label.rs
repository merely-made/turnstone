// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotClipStatus {
    pub fn label(&self) -> String {
        match self {
            Self::Ready => "ready".into(),
            Self::Sending => "clipping".into(),
            Self::Saved => "clip saved".into(),
            Self::Stale => "clip target changed; retry".into(),
            Self::Rejected(reason) => reason.clone(),
        }
    }
}
