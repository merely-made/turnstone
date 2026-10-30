// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SkyEngineV1 {
    pub(super) fn turquet() -> Self {
        Self {
            package: TURQUET_PACKAGE.to_owned(),
            version: TURQUET_VERSION.to_owned(),
            source_revision: TURQUET_SOURCE_REVISION.to_owned(),
        }
    }

    pub(super) fn validate(&self) -> Result<(), SkyReceiptJsonError> {
        validate_nonempty("Sky engine package", &self.package)?;
        validate_nonempty("Sky engine version", &self.version)?;
        validate_nonempty("Sky engine source revision", &self.source_revision)?;
        if self.package != TURQUET_PACKAGE
            || self.version != TURQUET_VERSION
            || self.source_revision != TURQUET_SOURCE_REVISION
        {
            return Err(invalid(format!(
                "Sky receipt V1 requires {TURQUET_PACKAGE} {TURQUET_VERSION} at {TURQUET_SOURCE_REVISION}"
            )));
        }
        Ok(())
    }
}
