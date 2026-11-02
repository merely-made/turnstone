// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

pub(in super::super) fn app_document_capabilities(
    capabilities: inker::DocumentCapabilities,
) -> crate::content::DocumentCapabilityFacts {
    use crate::content::{CapabilityStatus, DocumentCapabilityFacts};

    fn status(value: inker::DocumentCapabilityStatus) -> CapabilityStatus {
        match value {
            inker::DocumentCapabilityStatus::Supported => CapabilityStatus::Supported,
            inker::DocumentCapabilityStatus::Unsupported { reason } => {
                CapabilityStatus::Unsupported { reason }
            },
            inker::DocumentCapabilityStatus::Partial { detail } => {
                CapabilityStatus::Partial { detail }
            },
        }
    }

    DocumentCapabilityFacts {
        find_in_page: status(capabilities.find_in_page),
        page_zoom: status(capabilities.page_zoom),
        page_capture: status(capabilities.page_capture),
        navigation: status(capabilities.navigation),
    }
}
