// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// The omnibar as observed: state plus suggestion rows as display strings.
#[derive(Clone, Debug, PartialEq)]
pub struct OmnibarView {
    pub open: bool,
    pub text: String,
    pub cursor: usize,
    pub selected: usize,
    pub suggestions: Vec<String>,
}
