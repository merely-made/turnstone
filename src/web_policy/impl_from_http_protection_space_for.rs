// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl From<&HttpProtectionSpace> for ProtectionSpaceKey {
    fn from(space: &HttpProtectionSpace) -> Self {
        Self {
            host: space.host.to_ascii_lowercase(),
            port: space.port,
            realm: space.realm.clone(),
            scheme: space.scheme.to_ascii_lowercase(),
            is_proxy: space.is_proxy,
        }
    }
}
