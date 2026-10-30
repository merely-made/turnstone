// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The cambium event views return intents through `OptionalAction`, which
/// asks for this marker.
impl cambium::Action for ArrangeIntent {}
