// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl CredentialProvider {
    pub fn insert(&mut self, space: HttpProtectionSpace, credentials: HttpCredentials) {
        self.credentials
            .insert(ProtectionSpaceKey::from(&space), credentials);
    }

    pub fn get(&self, space: &HttpProtectionSpace) -> Option<&HttpCredentials> {
        self.credentials.get(&ProtectionSpaceKey::from(space))
    }

    pub fn remove(&mut self, space: &HttpProtectionSpace) {
        self.credentials.remove(&ProtectionSpaceKey::from(space));
    }
}
