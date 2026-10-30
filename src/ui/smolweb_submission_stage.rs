// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmolwebSubmissionStage {
    Body,
    Mime,
    Token,
    Confirm,
}
