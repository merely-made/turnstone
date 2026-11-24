// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

#[cfg(target_os = "macos")]
fn main() {
    let code = run().unwrap_or_else(|error| {
        eprintln!("Turnstone Weld helper: {error}");
        1
    });
    std::process::exit(code);
}

#[cfg(target_os = "macos")]
fn run() -> Result<i32, Box<dyn std::error::Error>> {
    use welding::{CefRuntime, CefSandboxMode};

    let executable = std::env::current_exe()?;
    let loader = cef::library_loader::LibraryLoader::new(&executable, true);
    if !loader.load() {
        return Err("could not load the CEF framework from the app bundle".into());
    }
    let sandbox = match std::env::var("TURNSTONE_WELD_SANDBOX").as_deref() {
        Ok("unsandboxed") => CefSandboxMode::UnsandboxedTrustedContent,
        Ok("sandboxed") | Err(std::env::VarError::NotPresent) => CefSandboxMode::Sandboxed,
        _ => return Err("invalid TURNSTONE_WELD_SANDBOX".into()),
    };
    Ok(CefRuntime::try_run_subprocess(
        &cef::args::Args::new(),
        sandbox,
    )?)
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Turnstone's Weld helper is a macOS app-bundle executable");
    std::process::exit(1);
}
