// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct UserAgentDecisionView {
    pub node: Uuid,
    pub request: u64,
    pub kind: &'static str,
    pub prompt: String,
    pub queued: usize,
    pub submitting: bool,
    pub authentication_field: Option<&'static str>,
    pub remember_for_process: bool,
    pub error: Option<String>,
}
