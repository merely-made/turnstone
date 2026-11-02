// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Real same-host receiving: Pelt/Genet and Graft/Servo through Webcue and
//! WgpuCue. `cargo run --features servo --example cue_surfaces -- OUTPUT_DIR [--offscreen]`
//! The finite, isolated fixture writes capability reports, events and pixels.
//! Pixel readback is queued and polled on later turns, without GPU waits.

#[cfg(target_os = "macos")]
#[path = "cue_surfaces/native.rs"]
mod native;

#[path = "cue_surfaces/main.rs"]
#[cfg(not(target_os = "macos"))]
mod main;
#[cfg(not(target_os = "macos"))]
use main::main;
#[path = "cue_surfaces/main_2.rs"]
#[cfg(target_os = "macos")]
mod main_2;
#[cfg(target_os = "macos")]
use main_2::main;
