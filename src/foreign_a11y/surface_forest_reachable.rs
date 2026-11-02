// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SurfaceForest {
    /// Validate ownership edges, including as-yet missing declared children.
    pub(super) fn reachable(&self) -> Result<HashSet<TreeId>, String> {
        let mut found = HashSet::new();
        let mut pending = vec![self.root];
        while let Some(id) = pending.pop() {
            if id == TreeId::ROOT || !found.insert(id) {
                return Err(
                    "foreign tree graph has a root collision, cycle or multiple parents".into(),
                );
            }
            if let Some(tree) = self.trees.get(&id) {
                pending.extend(tree.children().into_iter().rev());
            }
        }
        Ok(found)
    }

    pub(super) fn ready(&self) -> bool {
        self.declared.iter().all(|id| self.trees.contains_key(id))
    }

    pub(super) fn reset_publication(&mut self) {
        for tree in self.trees.values_mut() {
            tree.publication = Grafts::new();
        }
    }

    pub(super) fn prepare(&mut self, id: TreeId) -> Result<Vec<TreeUpdate>, String> {
        let tree = self.trees.get(&id).ok_or("foreign forest is incomplete")?;
        let snapshot = tree.snapshot(id);
        let children = tree.children();
        let mut frames = HashMap::new();
        let mut guests = Vec::new();
        for child in children {
            let frame = self.prepare(child)?;
            guests.push((child, frame[0].clone()));
            frames.insert(child, frame);
        }
        let composition = Composition::new(snapshot, guests).map_err(|error| error.to_string())?;
        let updates = self
            .trees
            .get_mut(&id)
            .unwrap()
            .publication
            .frame(composition);
        // Each child placeholder is its safe first parent update. Replace it
        // with the child's complete frame, so descendants and their focus are
        // installed before an ancestor's final focus-only move reaches them.
        Ok(expand(updates, &frames))
    }
}
