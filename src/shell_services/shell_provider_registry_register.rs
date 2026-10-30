// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ShellProviderRegistry {
    pub fn register(
        &mut self,
        provider: ShellProviderRegistration,
    ) -> Option<ShellProviderRegistration> {
        self.providers.insert(provider.id.clone(), provider)
    }

    pub fn unregister(&mut self, id: &ShellProviderId) -> Option<ShellProviderRegistration> {
        self.providers.remove(id)
    }

    pub fn get(&self, id: &ShellProviderId) -> Option<&ShellProviderRegistration> {
        self.providers.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ShellProviderRegistration> {
        self.providers.values()
    }
}
