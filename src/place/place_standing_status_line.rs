// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceStanding {
    /// The status row naming a withdrawal, if there is one.
    pub fn status_line(self) -> Option<String> {
        match self {
            Self::Member => None,
            Self::MembershipRevoked => Some("Membership: revoked".into()),
            Self::GrantRevoked => Some("Grant: revoked".into()),
            Self::GrantExpired { at_ms } => Some(format!("Grant: expired at {}", utc_ms(at_ms))),
        }
    }

    /// Why a write or dial is refused, as a clause.
    pub fn refusal(self) -> Option<String> {
        match self {
            Self::Member => None,
            Self::MembershipRevoked => Some("its place membership was revoked".into()),
            Self::GrantRevoked => Some("its grant was revoked".into()),
            Self::GrantExpired { at_ms } => Some(format!("its grant expired at {}", utc_ms(at_ms))),
        }
    }
}
