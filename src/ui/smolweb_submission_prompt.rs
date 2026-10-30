// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// App-owned state for one explicit write conversation. Request bytes are
/// retained separately from the visible line as the composer advances.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmolwebSubmissionPrompt {
    pub source: Option<Uuid>,
    pub target: String,
    pub protocol: SmolwebSubmissionProtocol,
    pub stage: SmolwebSubmissionStage,
    pub body: Vec<u8>,
    pub file_name: Option<String>,
    pub mime: String,
    pub token: Option<crate::action::SensitiveString>,
}
