// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Uninstall: forget a project and keep its files (mod projects MP7c, D61).

use super::{ModProjectHost, lock};

impl ModProjectHost {
    /// Take project `id` off the list: out of `installed.json` with its grants and claimed
    /// media types, so a parser's types go back to the built-in engines, and detached if it was
    /// attached. The folder and its last-good build stay on disk; opening the folder again
    /// installs it afresh (nothing it was granted or claimed comes back).
    ///
    /// The runtime's in-memory install record stays until the next admission replaces it.
    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        let mut state = lock(&self.state);
        let project = state
            .projects
            .remove(id)
            .ok_or_else(|| format!("no mod project {id}"))?;
        if let Some(page) = project.page {
            let _ = page.script.detach();
        }
        if let Some(script) = project.script {
            let _ = script.detach();
        }
        self.save_list(&state);
        Ok(())
    }
}
