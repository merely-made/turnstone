// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn run() -> std::result::Result<(), String> {
    let output = std::env::args_os()
        .nth(1)
        .ok_or("supply a fresh output directory")?;
    let output = PathBuf::from(output);
    if output.exists() {
        return Err("output directory must be fresh".into());
    }
    let output = std::path::absolute(output).map_err(|e| e.to_string())?;
    let require_window = match std::env::args_os().nth(2) {
        None => true,
        Some(flag) if flag == "--offscreen" => false,
        Some(_) => return Err("only --offscreen is accepted after the output directory".into()),
    };
    let mut app = App::new(output, require_window);
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    let mut builder = EventLoop::builder();
    builder
        .with_activation_policy(ActivationPolicy::Regular)
        .with_activate_ignoring_other_apps(require_window);
    let el = builder.build().map_err(|e| e.to_string())?;
    el.run_app(&mut app).map_err(|e| e.to_string())?;
    if let Some(e) = app.error {
        return Err(e);
    }
    if !app.passed {
        return Err("receiver closed before qualification completed".into());
    }
    Ok(())
}
