// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl commons::CommonsAuthority for EveryOperation {
    fn classify(
        &self,
        _: servitor::Subject,
        _: &servitor::Cap,
        _: servitor::Mode,
    ) -> commons::AuthorityState {
        commons::AuthorityState::Revoked
    }
}
