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
    let out = Rc::new(RefCell::new(None));
    let app = App::new(output, require_window, out.clone());
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    let mut builder = EventLoop::builder();
    builder
        .with_activation_policy(ActivationPolicy::Regular)
        .with_activate_ignoring_other_apps(require_window);
    let el = builder.build().map_err(|e| e.to_string())?;
    el.run_app(app).map_err(|e| e.to_string())?;
    let (error, passed) = out
        .borrow_mut()
        .take()
        .ok_or("the event loop did not drop the app")?;
    if let Some(e) = error {
        return Err(e);
    }
    if !passed {
        return Err("receiver closed before qualification completed".into());
    }
    Ok(())
}
