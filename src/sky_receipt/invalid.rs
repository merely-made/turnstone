// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn invalid(message: impl Into<String>) -> SkyReceiptJsonError {
    SkyReceiptJsonError::Invalid(message.into())
}
