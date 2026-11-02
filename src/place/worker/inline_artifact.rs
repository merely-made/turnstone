// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Wrap bytes as a digest-checked inline artifact.
pub(super) fn inline_artifact(bytes: &[u8]) -> crate::place::invite::ArtifactRefV1 {
    crate::place::invite::ArtifactRefV1::Inline {
        media_type: "application/vnd.mere.place-artifact".into(),
        digest: *Digest::blake3(bytes)
            .bytes
            .first_chunk::<32>()
            .expect("blake3 produces 32 bytes"),
        bytes: bytes.to_vec(),
    }
}
