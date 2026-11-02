// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlaceCardV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PLACE_CARD_VERSION {
            return Err(format!("unsupported place card version {}", self.version));
        }
        self.binding.validate().map_err(|error| error.to_string())?;
        decode_hex("founder root", &self.founder_root).map_err(|error| error.to_string())?;
        Ok(())
    }
}
