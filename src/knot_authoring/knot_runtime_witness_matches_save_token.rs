// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotRuntimeWitness {
    /// Check that the endpoint still recognizes the observed save token.
    ///
    /// The caller receives only the comparison result, never the token.
    pub fn matches_save_token(&self, candidate: &[u8]) -> bool {
        self.base_token == candidate
    }
}
