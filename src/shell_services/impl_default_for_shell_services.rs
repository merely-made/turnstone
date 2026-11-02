// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for ShellServices {
    fn default() -> Self {
        let mut providers = ShellProviderRegistry::default();
        providers.register(ShellProviderRegistration {
            id: ShellProviderId::new("turnstone.actions"),
            label: "Turnstone actions".to_string(),
            kinds: vec![ShellProviderKind::Command],
        });
        providers.register(ShellProviderRegistration {
            id: ShellProviderId::new("turnstone.navigation"),
            label: "Turnstone navigation".to_string(),
            kinds: vec![ShellProviderKind::Navigation, ShellProviderKind::Completion],
        });
        Self {
            chrome: ShellChromeConfig::default(),
            providers,
            transcript: ShellTranscript::default(),
            omnibar_context: None,
            requested_context: None,
        }
    }
}
