// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resolution_error(error: ResolutionError) -> String {
    format!("could not resolve Turnstone presentation: {error:?}")
}
