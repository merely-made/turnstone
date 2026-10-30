// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PlacePrekeyOfferV1 {
    /// Bounded, decoded bundle bytes, or the reason they cannot be read.
    pub fn prekey_bytes(&self) -> Result<Vec<u8>, String> {
        use base64::Engine as _;
        if self.version != PLACE_CARD_VERSION {
            return Err(format!(
                "unsupported pre-key offer version {}",
                self.version
            ));
        }
        decode_hex("offering root", &self.root).map_err(|error| error.to_string())?;
        base64::engine::general_purpose::STANDARD
            .decode(&self.prekey)
            .map_err(|error| format!("decode pre-key bundle: {error}"))
    }
}
