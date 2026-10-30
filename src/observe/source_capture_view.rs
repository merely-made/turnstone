// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct SourceCaptureView {
    pub request: Uuid,
    pub ordinal: u64,
    pub session: crate::panes::SessionId,
    pub graph: crate::panes::GraphId,
    pub node: Uuid,
    pub url: String,
    pub source: Option<crate::source_capture::SourceIdentity>,
    pub keep: bool,
    pub phase: crate::source_capture::CapturePhase,
    pub error: Option<String>,
    pub stored: Option<crate::action::StoredSourceDocument>,
}
