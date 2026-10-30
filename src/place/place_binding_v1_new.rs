// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceBindingV1 {
    pub fn new(
        moot: PlaceId,
        root: SharedContainerId,
        chat: ChatSpaceId,
        default_channel: impl Into<String>,
    ) -> Result<Self, PlaceBindingError> {
        let binding = Self {
            version: PLACE_BINDING_VERSION,
            moot,
            root,
            chat,
            default_channel: default_channel.into(),
        };
        binding.validate()?;
        Ok(binding)
    }

    /// Validate product-level bounds after deserialization. Domain membership
    /// and authority are intentionally not guessed here.
    pub fn validate(&self) -> Result<(), PlaceBindingError> {
        if self.version != PLACE_BINDING_VERSION {
            return Err(PlaceBindingError::UnsupportedVersion(self.version));
        }
        if self.default_channel.trim().is_empty() {
            return Err(PlaceBindingError::EmptyDefaultChannel);
        }
        if self.default_channel.len() > 128 {
            return Err(PlaceBindingError::DefaultChannelTooLong(
                self.default_channel.len(),
            ));
        }
        if self.default_channel.chars().any(char::is_control) {
            return Err(PlaceBindingError::DefaultChannelHasControl);
        }
        Ok(())
    }
}
