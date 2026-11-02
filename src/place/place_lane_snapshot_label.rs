// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceLaneSnapshot {
    pub(super) fn label(&self) -> &str {
        match self.name {
            "gemot/constitution/v1" => "Constitution",
            "gemot/delegation/v1" => "Delegation",
            "gemot/membership/v1" => "Membership",
            "gemot/records/v1" => "Records",
            "gemot/standing/v1" => "Standing",
            "gemot/tulpa/v1" => "Tulpa",
            "gemot/flora/v1" => "Flora",
            "commons/graph/encrypted/v1" => "Shared graph",
            "commons/chat/v1" => "Chat",
            "stickleback/group-key/v1" => "Group keys",
            other => other,
        }
    }
}
