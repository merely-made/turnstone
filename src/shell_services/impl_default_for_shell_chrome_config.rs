// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Default for ShellChromeConfig {
    fn default() -> Self {
        Self {
            layout: Default::default(),
            omnibar: OmnibarConfig::default(),
            shellbar: ShellbarConfig::default(),
            appearance: AppearanceConfig::default(),
            titlebar_insets: BTreeMap::new(),
            layout_scales: BTreeMap::new(),
            transcript_placement: ChromePlacement::Hidden,
            transcript_retention: TranscriptRetention::default(),
        }
    }
}
