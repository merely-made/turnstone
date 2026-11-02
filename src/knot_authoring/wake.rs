// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) type Wake = Arc<dyn Fn() + Send + Sync>;
