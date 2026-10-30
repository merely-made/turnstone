// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceWorkerSettings {
    /// What a live place needs to serve its vault, or `None` when it has none.
    pub(crate) fn projection_setup(
        &self,
        binding: &PlaceBindingV1,
        identity: &dyn IdentityProvider,
    ) -> Option<crate::place::projection_host::ProjectionSetup> {
        let root = self.knot_root.clone()?;
        Some(crate::place::projection_host::ProjectionSetup {
            root,
            max_source_bytes: self.knot_max_source_bytes,
            moot: binding.moot.0,
            authority: root_grant_id(binding.moot.0),
            issuer: identity.master_public_key().to_bytes(),
            clock: self.authority_clock,
        })
    }
}
