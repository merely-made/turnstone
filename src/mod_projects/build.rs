// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The build adapter (D1): cargo, where a toolchain is found.

use std::sync::Arc;

use document_host::CargoBuilder;

use super::ModProjectHost;

impl ModProjectHost {
    /// Build the project with the toolchain adapter and overwrite its component
    /// with the result. Blocks; the app calls [`build`](Self::build).
    pub fn build_blocking(&self, id: &str) -> Result<String, String> {
        let root = self.with(id, |p, _| {
            if p.building {
                return Err(format!("{id} is already building"));
            }
            p.building = true;
            Ok(p.root.clone())
        })?;
        let builder = CargoBuilder::new()
            .with_cargo(self.cargo.clone())
            .with_target_dir(root.join("target"));
        let built = self.runtime.build_project(id, &builder, &root);
        self.with(id, |p, state| {
            p.building = false;
            let outcome = built.map_err(|e| e.words()).and_then(|b| {
                std::fs::copy(&b.path, &p.component)
                    .map(|bytes| format!("built {} ({bytes} bytes)", p.component.display()))
                    .map_err(|e| format!("could not place {}: {e}", b.path.display()))
            });
            p.build_note = Some(match &outcome {
                Ok(note) => note.clone(),
                Err(why) => why.clone(),
            });
            outcome.map_err(|why| {
                let line = format!("{id}: build: {why}");
                Self::note_in(state, line.clone());
                p.failure = Some(format!("build: {why}"));
                line
            })
        })
    }

    /// Build on a worker thread; the listing shows the outcome when it lands.
    pub fn build(self: &Arc<Self>, id: &str) -> Result<(), String> {
        if !self.toolchain_present() {
            let line = format!("{id}: build: no toolchain ({} was not found)", self.cargo.display());
            self.note(line.clone());
            return Err(line);
        }
        let (host, id) = (self.clone(), id.to_string());
        std::thread::spawn(move || {
            let _ = host.build_blocking(&id);
        });
        Ok(())
    }
}
