// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl fmt::Display for SkyReceiptJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(source) => write!(formatter, "invalid Sky receipt JSON: {source}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported Sky receipt version {version}")
            },
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}
