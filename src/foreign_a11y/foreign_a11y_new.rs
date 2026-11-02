// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl ForeignA11y {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Share validated producer state while each native window keeps its own
    /// publication history. Supplier TreeIds and local NodeIds stay intact.
    pub(crate) fn sync_retained_from(&mut self, source: &Self) {
        let previous = std::mem::take(&mut self.surfaces);
        self.surfaces = source.surfaces.clone();
        self.owners = source.owners.clone();
        for (surface, forest) in &mut self.surfaces {
            let old = previous.get(surface).filter(|old| old.root == forest.root);
            for (id, tree) in &mut forest.trees {
                tree.publication = old
                    .and_then(|old| old.trees.get(id))
                    .map(|tree| tree.publication.clone())
                    .unwrap_or_default();
            }
        }
        self.mounted.retain(|surface| {
            self.surfaces.get(surface).is_some_and(|current| {
                current.ready()
                    && previous
                        .get(surface)
                        .is_some_and(|old| old.root == current.root)
            })
        });
        self.published.retain(|surface, published| {
            self.surfaces
                .get(surface)
                .is_some_and(|current| current.ready() && current.root == published.root)
        });
        self.dirty = true;
    }

    pub(crate) fn admit(&mut self, surface: Uuid, root: TreeId) -> Result<(), String> {
        if root == TreeId::ROOT || self.owners.get(root).is_some_and(|owner| *owner != surface) {
            return Err("foreign root is owned by the host or another surface".into());
        }
        if let Some(current) = self.surfaces.get(&surface) {
            if current.root == root {
                return Ok(());
            }
            // A root cannot be promoted from its own old nested namespace.
            if current.declared.contains(&root) {
                return Err("replacement root is already an old nested tree".into());
            }
        }
        self.retire(surface);
        self.owners.insert(root, surface);
        self.surfaces.insert(
            surface,
            SurfaceForest {
                root,
                trees: HashMap::new(),
                declared: HashSet::from([root]),
            },
        );
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn retire(&mut self, surface: Uuid) {
        self.published.remove(&surface);
        if let Some(forest) = self.surfaces.remove(&surface) {
            for id in forest.declared {
                self.owners.remove(id);
            }
            self.mounted.remove(&surface);
            self.dirty = true;
        }
    }

    pub(crate) fn clear(&mut self) {
        self.published.clear();
        self.published_forest.clear();
        self.surfaces.clear();
        self.owners = GraftTable::new(TreeId::ROOT);
        self.mounted.clear();
        self.dirty = true;
    }

    pub(crate) fn apply(&mut self, surface: Uuid, update: TreeUpdate) -> Result<(), String> {
        let forest = self
            .surfaces
            .get(&surface)
            .ok_or("foreign surface is not admitted")?;
        // Unknown or stale callbacks cannot damage a current valid forest.
        if !forest.declared.contains(&update.tree_id) {
            return Err("foreign update names an unreferenced or stale tree".into());
        }
        let mut candidate = forest.clone();
        let update_id = update.tree_id;
        let tree = match RetainedTree::update(candidate.trees.get(&update_id), update) {
            Ok(tree) => tree,
            Err(error @ MaterializationError::Incomplete(_))
                if !candidate.trees.contains_key(&update_id) =>
            {
                // A late delta may be re-grafted under an upstream wrapper
                // after reactivation. None or Some(tree) does not prove a
                // complete initial graph. Wait without retaining those nodes
                // or admitting publication/actions until a valid full update.
                // This cannot distinguish a stale full callback from a fresh
                // one when the supplier reuses its document TreeId.
                return Err(error.to_string());
            },
            Err(error) => {
                self.retire(surface);
                return Err(error.to_string());
            },
        };
        let result = (|| {
            candidate.trees.insert(update_id, tree);
            let declared = candidate.reachable()?;
            if declared
                .iter()
                .any(|id| self.owners.get(*id).is_some_and(|owner| *owner != surface))
            {
                return Err("foreign tree is already owned by another surface".into());
            }
            candidate.trees.retain(|id, _| declared.contains(id));
            candidate.declared = declared;
            Ok(())
        })();
        if let Err(error) = result {
            // Malformed known-tree updates withdraw actions and publication.
            // The caller can explicitly resync once using a fresh root.
            self.retire(surface);
            return Err(error);
        }
        for old in &forest.declared {
            if !candidate.declared.contains(old) {
                self.owners.remove(*old);
            }
        }
        for id in &candidate.declared {
            self.owners.insert(*id, surface);
        }
        self.surfaces.insert(surface, candidate);
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn ready(&self, surface: Uuid) -> bool {
        self.surfaces
            .get(&surface)
            .is_some_and(SurfaceForest::ready)
    }

    pub(crate) fn graft(&self, surface: Uuid, rect: Rect) -> Option<Node> {
        let forest = self.surfaces.get(&surface)?;
        if !forest.ready()
            || ![rect.x0, rect.y0, rect.x1, rect.y1]
                .into_iter()
                .all(f64::is_finite)
            || rect.x1 <= rect.x0
            || rect.y1 <= rect.y0
        {
            return None;
        }
        Some(graft_node(
            Role::GenericContainer,
            forest.root,
            (rect.x0, rect.y0),
            (rect.x1 - rect.x0, rect.y1 - rect.y0),
        ))
    }

    pub(crate) fn needs_publish(&self) -> bool {
        self.dirty
    }

    pub(crate) fn reset_publication(&mut self) {
        self.published.clear();
        self.published_forest.clear();
        self.publication = Grafts::new();
        self.mounted.clear();
        for forest in self.surfaces.values_mut() {
            forest.reset_publication();
        }
        self.dirty = true;
    }

    /// An inactive platform has not accepted the composition yet. Rebuild
    /// the next ordered batch while retaining the exact in-process witness
    /// used by scenario observations; this grants no action admission.
    pub(crate) fn retry_publication(&mut self) {
        self.publication = Grafts::new();
        self.mounted.clear();
        for forest in self.surfaces.values_mut() {
            forest.reset_publication();
        }
        self.dirty = true;
    }

    pub(super) fn prepare(
        &self,
        host: TreeUpdate,
        visible: &[Uuid],
        initial: bool,
        local_guests: &[TreeUpdate],
    ) -> Result<
        (
            Composition,
            HashMap<TreeId, Vec<TreeUpdate>>,
            HashMap<Uuid, SurfaceForest>,
        ),
        String,
    > {
        if host.tree_id != TreeId::ROOT {
            return Err("foreign composition host must use ROOT".into());
        }
        RetainedTree::update(None, host.clone()).map_err(|error| error.to_string())?;
        let mut surfaces = self.surfaces.clone();
        let mut seen = HashSet::new();
        let mut frames = HashMap::new();
        let mut guests: Vec<_> = local_guests
            .iter()
            .map(|guest| (guest.tree_id, guest.clone()))
            .collect();
        for surface in visible {
            if !seen.insert(*surface) {
                return Err("visible surface occurs more than once".into());
            }
            let Some(forest) = surfaces.get_mut(surface).filter(|forest| forest.ready()) else {
                continue;
            };
            if initial || !self.mounted.contains(surface) {
                forest.reset_publication();
            }
            let frame = forest.prepare(forest.root)?;
            guests.push((forest.root, frame[0].clone()));
            frames.insert(forest.root, frame);
        }
        let composition = Composition::new(host, guests).map_err(|error| error.to_string())?;
        Ok((composition, frames, surfaces))
    }

    pub(crate) fn initial_host(
        &self,
        host: TreeUpdate,
        visible: &[Uuid],
    ) -> Result<TreeUpdate, String> {
        self.initial_host_with_guests(host, visible, &[])
    }

    pub(crate) fn initial_host_with_guests(
        &self,
        host: TreeUpdate,
        visible: &[Uuid],
        guests: &[TreeUpdate],
    ) -> Result<TreeUpdate, String> {
        let (composition, _, _) = self.prepare(host, visible, true, guests)?;
        Ok(composition.initial_host())
    }

    pub(crate) fn frame(
        &mut self,
        host: TreeUpdate,
        visible: &[Uuid],
    ) -> Result<Vec<TreeUpdate>, String> {
        self.frame_with_guests(host, visible, &[])
    }

    pub(crate) fn frame_with_guests(
        &mut self,
        host: TreeUpdate,
        visible: &[Uuid],
        guests: &[TreeUpdate],
    ) -> Result<Vec<TreeUpdate>, String> {
        let (composition, frames, surfaces) = self.prepare(host, visible, false, guests)?;
        let mut publication = self.publication.clone();
        let updates = expand(publication.frame(composition), &frames);
        self.surfaces = surfaces;
        self.publication = publication;
        self.mounted = visible
            .iter()
            .copied()
            .filter(|surface| self.ready(*surface))
            .collect();
        self.published = self
            .mounted
            .iter()
            .map(|surface| {
                let forest = &self.surfaces[surface];
                let ordered_updates: Vec<_> = updates
                    .iter()
                    .filter(|update| forest.declared.contains(&update.tree_id))
                    .cloned()
                    .collect();
                let mut trees: BTreeMap<TreeId, TreeUpdate> = BTreeMap::new();
                for update in &ordered_updates {
                    if !update.nodes.is_empty() {
                        trees.insert(update.tree_id, update.clone());
                    } else if let Some(snapshot) = trees.get_mut(&update.tree_id) {
                        snapshot.focus = update.focus;
                    }
                }
                (
                    *surface,
                    PublishedSurface {
                        root: forest.root,
                        trees,
                        ordered_updates,
                    },
                )
            })
            .collect();
        // Reconstruct the final whole snapshots from this exact ordered batch,
        // including the final host focus update. Observation reads this forest.
        let mut forest: BTreeMap<TreeId, TreeUpdate> = BTreeMap::new();
        for update in &updates {
            if update.tree.is_some() {
                forest.insert(update.tree_id, update.clone());
            } else if let Some(tree) = forest.get_mut(&update.tree_id) {
                tree.focus = update.focus;
                for (id, node) in &update.nodes {
                    if let Some((_, previous)) = tree.nodes.iter_mut().find(|(n, _)| n == id) {
                        *previous = node.clone();
                    } else {
                        tree.nodes.push((*id, node.clone()));
                    }
                }
            }
        }
        self.published_forest = forest.into_values().collect();
        self.dirty = false;
        Ok(updates)
    }

    pub(crate) fn observed_lines(&self) -> Vec<String> {
        crate::a11y::forest_lines(&self.published_forest)
    }

    pub(super) fn published_surfaces(&self) -> Vec<(Uuid, &PublishedSurface)> {
        let mut surfaces: Vec<_> = self
            .published
            .iter()
            .filter(|(surface, published)| {
                self.mounted.contains(*surface)
                    && self
                        .surfaces
                        .get(*surface)
                        .is_some_and(|forest| forest.ready() && forest.root == published.root)
            })
            .map(|(surface, published)| (*surface, published))
            .collect();
        surfaces.sort_by_key(|(surface, _)| *surface);
        surfaces
    }

    pub(crate) fn published_counts(&self) -> (usize, usize, usize) {
        let surfaces = self.published_surfaces();
        let trees = surfaces
            .iter()
            .map(|(_, surface)| surface.trees.len())
            .sum();
        let nodes = surfaces
            .iter()
            .flat_map(|(_, surface)| surface.trees.values())
            .map(|tree| tree.nodes.len())
            .sum();
        (surfaces.len(), trees, nodes)
    }

    /// Actual labels/values from the successful composed frame. The caller
    /// must opt into known public fixture content before observing these.
    pub(crate) fn published_text(&self) -> String {
        self.published_surfaces()
            .iter()
            .flat_map(|(_, surface)| surface.trees.values())
            .flat_map(|tree| {
                let protected = protected_nodes(tree);
                tree.nodes
                    .iter()
                    .filter(move |(id, _)| !protected.contains(id))
                    .flat_map(|(_, node)| [node.label(), node.value()])
                    .flatten()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
