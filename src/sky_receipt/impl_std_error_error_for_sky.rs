// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl std::error::Error for SkyReceiptJsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(source) => Some(source),
            Self::UnsupportedVersion(_) | Self::Invalid(_) => None,
        }
    }
}
