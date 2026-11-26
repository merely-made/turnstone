// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// One step of a Wasm mod project (mod projects MP5). A project is named by its
/// mod id; `dev` selects dev-mode admission over conformant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModProjectAction {
    /// Copy the project folder `from` (the starter when `None`) in and open it.
    Create { from: Option<String> },
    /// Open the project folder at `path`.
    Open { path: String, dev: bool },
    Check(String),
    Build(String),
    /// Attach the project's component to its document.
    Attach(String),
    RunFixtures(String),
    /// Re-admit the edited component and swap it in on the same document.
    Reload(String),
    Detach(String),
    SetLevel { id: String, dev: bool },
    /// Deliver one event to the attached component.
    Turn { id: String, kind: String, payload: String },
}
