// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Bole evaluates scripts on the window thread. Match Ortet's stack
    // reserve for debug builds on Windows, where the default is 1 MiB.
    if std::env::var_os("CARGO_FEATURE_SCRIPTED_WEBGL").is_some()
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bin=turnstone=/STACK:16777216");
    }
}
