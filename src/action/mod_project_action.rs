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
    /// Attach the project's component to the focused Livery page (MP5b, D14, D25).
    AttachPage(String),
    RunFixtures(String),
    /// Re-admit the edited component and swap it in on the same document.
    Reload(String),
    /// Put the last passing component back and reload through the pin (MP6, D30, D31).
    Rollback(String),
    Detach(String),
    SetLevel { id: String, dev: bool },
    /// Grant or revoke a capability the manifest declares (next attach or reload).
    SetGrant { id: String, capability: String, granted: bool },
    /// Hand a parser mod a media type a built-in also claims, or take it back (MP7, D39).
    SetClaimed { id: String, media_type: String, on: bool },
    /// Deliver one event to the attached component.
    Turn { id: String, kind: String, payload: String },
}
