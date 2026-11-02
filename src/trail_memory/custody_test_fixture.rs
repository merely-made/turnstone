// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

struct Sealer;
impl eidetic::PayloadSealer for Sealer {
    fn seal(
        &self,
        _: &eidetic::Hash,
        clear: &[u8],
    ) -> eidetic::Result<(Vec<u8>, eidetic::SealedBlobRef)> {
        Ok((
            clear.to_vec(),
            eidetic::SealedBlobRef {
                epoch: eidetic::SealEpochId([1; 16]),
                format: "test-fixture".into(),
            },
        ))
    }

    fn unseal(
        &self,
        _: &eidetic::Hash,
        _: &eidetic::SealedBlobRef,
        ciphertext: &[u8],
    ) -> eidetic::Result<Vec<u8>> {
        Ok(ciphertext.to_vec())
    }
}

pub(crate) fn custody() -> super::SourceCustody {
    super::SourceCustody {
        sealer: std::sync::Arc::new(Sealer),
        epoch: std::sync::Arc::new(|| Some(1)),
    }
}
