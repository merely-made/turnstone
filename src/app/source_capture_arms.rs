// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Explicit capture admission and correlated persistence acknowledgement.
use super::App;
use crate::action::{Effect, StoredSourceDocument};
use crate::observe::AppEvent;
use crate::source_capture::{
    self, CaptureAttempt, CapturePhase, CaptureRequest, SourceCaptureTarget,
};

#[path = "source_capture_arms/app_await_source_capture_revalidation.rs"]
mod app_await_source_capture_revalidation;
#[path = "source_capture_arms/app_recover_source_capture.rs"]
mod app_recover_source_capture;
#[path = "source_capture_arms/app_retire_private_custody.rs"]
mod app_retire_private_custody;
