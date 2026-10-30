// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn inline_artifact(bytes: &[u8]) -> ArtifactRefV1 {
    ArtifactRefV1::Inline {
        media_type: "application/vnd.mere.place-artifact".into(),
        digest: proofs::Digest::blake3(bytes)
            .bytes
            .as_slice()
            .try_into()
            .unwrap(),
        bytes: bytes.to_vec(),
    }
}
