// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn viewing_profile() -> chirograph::CapabilityProfile {
    chirograph::CapabilityProfile::new([
        chirograph::PresentationCapability::EditableText,
        chirograph::PresentationCapability::PortableCard,
    ])
}
