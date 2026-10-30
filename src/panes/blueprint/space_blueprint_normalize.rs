// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl SpaceBlueprint {
    pub fn normalize(&mut self) {
        let known: HashSet<_> = self.panes.iter().map(|pane| pane.id).collect();
        self.tiled = self
            .tiled
            .take()
            .and_then(|tree| tree.normalized(&known, self.normalization));
    }

    /// All pane ids in the tiled topology, including inactive tab children.
    /// They still own one station and retained state even while hidden.
    pub fn tiled_panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        if let Some(tree) = &self.tiled {
            tree.collect_panes(&mut panes);
        }
        panes
    }

    /// The tiled panes whose surfaces are currently live. Hosts use this for
    /// rendering, hit testing, accessibility focus, and pumping; inactive
    /// tab children are intentionally absent.
    pub fn active_tiled_panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        if let Some(tree) = &self.tiled {
            tree.collect_active_panes(&mut panes);
        }
        panes
    }

    pub fn pane(&self, id: PaneId) -> Option<&PaneSpec> {
        self.panes.iter().find(|pane| pane.id == id)
    }

    /// Move one tiled pane beside another. This is the topology half of a
    /// pane drag; the host keeps renderer state keyed by the unchanged id.
    pub fn move_pane_beside(
        &mut self,
        pane: PaneId,
        target: PaneId,
        axis: SplitAxis,
        after: bool,
    ) -> bool {
        if pane == target
            || !self.tiled_panes().contains(&pane)
            || !self.tiled_panes().contains(&target)
        {
            return false;
        }
        let Some(mut tree) = self.tiled.take().and_then(|tree| tree.without_pane(pane)) else {
            return false;
        };
        if !tree.insert_beside(target, pane, axis, after) {
            return false;
        }
        self.tiled = Some(tree);
        self.normalize();
        true
    }

    /// Move one tiled pane into a tabbed subtree over `target` and select it.
    pub fn stack_pane_onto(&mut self, pane: PaneId, target: PaneId) -> bool {
        if pane == target
            || !self.tiled_panes().contains(&pane)
            || !self.tiled_panes().contains(&target)
        {
            return false;
        }
        let Some(mut tree) = self.tiled.take().and_then(|tree| tree.without_pane(pane)) else {
            return false;
        };
        if !tree.insert_tab(target, pane) {
            return false;
        }
        self.tiled = Some(tree);
        self.normalize();
        true
    }

    /// Select the tab containing `pane`. The pane remains in the same station;
    /// only its visibility and active surface lifecycle change.
    pub fn activate_tab(&mut self, pane: PaneId) -> bool {
        self.tiled
            .as_mut()
            .is_some_and(|tree| tree.activate_tab_containing(pane))
    }

    /// Apply a complete N-ary split weighting at `path`. Fractions are
    /// normalized as one topology edit, so a resize cannot leave a zero-width
    /// sibling behind.
    pub fn set_split_fractions(&mut self, path: &[LayoutPathStep], fractions: &[f32]) -> bool {
        self.tiled
            .as_mut()
            .is_some_and(|tree| tree.set_split_fractions(path, fractions))
    }

    /// Receive a pane that was torn out of another space as this space's tiled
    /// root. The pane id and its spec are transferred intact; A5 adds the
    /// floating station and window geometry around this same operation.
    pub fn insert_tiled_root(&mut self, spec: PaneSpec) -> Result<(), BlueprintViolation> {
        if self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        if self.tiled.is_some() || !self.floating.is_empty() {
            return Err(BlueprintViolation::DuplicatePaneStation(spec.id));
        }
        let pane = spec.id;
        self.panes.push(spec);
        self.tiled = Some(LayoutNode::Pane(pane));
        Ok(())
    }

    /// Insert a new tiled pane beside an existing station, preserving the
    /// caller's pane id, source, and config as one blueprint operation. The
    /// topology is changed only after all identity checks pass.
    pub fn insert_tiled_beside(
        &mut self,
        spec: PaneSpec,
        target: PaneId,
        axis: SplitAxis,
        after: bool,
    ) -> Result<(), BlueprintViolation> {
        if self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        if self.station_ids().contains(&spec.id) {
            return Err(BlueprintViolation::DuplicatePaneStation(spec.id));
        }
        if !self.tiled_panes().contains(&target) {
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane: target,
            });
        }
        let pane = spec.id;
        let Some(mut tree) = self.tiled.take() else {
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane: target,
            });
        };
        if !tree.insert_beside(target, pane, axis, after) {
            self.tiled = Some(tree);
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane: target,
            });
        }
        self.tiled = Some(tree);
        self.panes.push(spec);
        self.normalize();
        Ok(())
    }

    /// Floats that are live in the current window. Hiding the float layer
    /// suppresses ordinary floats, but a pinned float remains visible by
    /// definition. The host owns the toggle because it is transient window
    /// presentation state, not a second layout tree.
    pub fn visible_floating_panes(&self, float_layer_visible: bool) -> Vec<&FloatingPane> {
        let mut floats: Vec<_> = self
            .floating
            .iter()
            .filter(|item| item.visible && (float_layer_visible || item.pinned))
            .collect();
        floats.sort_by_key(|item| (item.z, item.pane.0));
        floats
    }

    pub fn float_pane(&mut self, pane: PaneId, rect: RelativeRect) -> bool {
        if !self.panes.iter().any(|spec| spec.id == pane) {
            return false;
        }
        let old_float = self.floating.iter().find(|item| item.pane == pane).cloned();
        self.floating.retain(|item| item.pane != pane);
        if let Some(tree) = self.tiled.take() {
            self.tiled = tree.without_pane(pane);
        }
        let z = self
            .floating
            .iter()
            .map(|item| item.z)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.floating.push(FloatingPane {
            pane,
            rect,
            constraints: old_float
                .as_ref()
                .map(|item| item.constraints)
                .unwrap_or_default(),
            z,
            pinned: old_float.as_ref().is_some_and(|item| item.pinned),
            visible: old_float.as_ref().is_none_or(|item| item.visible),
        });
        self.normalize();
        true
    }

    /// Update the proportional placement without changing the pane's station
    /// or z-order. A shell calls this while a float drag is in flight and
    /// persists once the gesture settles.
    pub fn set_float_rect(&mut self, pane: PaneId, rect: RelativeRect) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.rect = rect;
        true
    }

    pub fn set_float_constraints(
        &mut self,
        pane: PaneId,
        constraints: FloatSizeConstraints,
    ) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.constraints = constraints;
        true
    }

    pub fn set_float_pinned(&mut self, pane: PaneId, pinned: bool) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.pinned = pinned;
        true
    }

    pub fn set_float_visible(&mut self, pane: PaneId, visible: bool) -> bool {
        let Some(float) = self.floating.iter_mut().find(|item| item.pane == pane) else {
            return false;
        };
        float.visible = visible;
        true
    }

    /// Raise the clicked float above every other float in this space. Equal or
    /// stale persisted z-values are normalized first so order is deterministic
    /// and the counter cannot drift indefinitely across saves.
    pub fn raise_float(&mut self, pane: PaneId) -> bool {
        if !self.floating.iter().any(|item| item.pane == pane) {
            return false;
        }
        self.normalize_float_z();
        let index = self
            .floating
            .iter()
            .position(|item| item.pane == pane)
            .expect("float was checked before z normalization");
        let z = self.floating.len() as u32 + 1;
        self.floating[index].z = z;
        self.normalize_float_z();
        true
    }

    /// Dock a floating pane back into the serializable tiled topology. The
    /// pane spec remains in this space and keeps the same identity; only its
    /// station changes.
    pub fn dock_floating_pane(&mut self, pane: PaneId, target: FloatDockTarget) -> bool {
        let Some(float_index) = self.floating.iter().position(|item| item.pane == pane) else {
            return false;
        };
        let docked = match target {
            FloatDockTarget::TiledRoot => {
                if self.tiled.is_some() {
                    return false;
                }
                self.tiled = Some(LayoutNode::Pane(pane));
                true
            },
            FloatDockTarget::Beside {
                target,
                axis,
                after,
            } => self
                .tiled
                .as_mut()
                .is_some_and(|tree| tree.insert_beside(target, pane, axis, after)),
            FloatDockTarget::Tab { target } => self
                .tiled
                .as_mut()
                .is_some_and(|tree| tree.insert_tab(target, pane)),
        };
        if !docked {
            return false;
        }
        self.floating.remove(float_index);
        self.normalize();
        true
    }

    /// Transfer one floating station to another OS-window space, preserving
    /// its relative rectangle, constraints, pin, visibility, spec, and
    /// therefore its `PaneId`-keyed retained runner. The destination gives it
    /// its own topmost z-order because float order is scoped to a window.
    pub fn tear_out_floating_pane(
        &mut self,
        pane: PaneId,
        destination: &mut SpaceBlueprint,
    ) -> Result<(), BlueprintViolation> {
        // Check the destination before taking anything from the source. A
        // rejected tear-out must leave the originating pane and its runner
        // lookup reachable where they started.
        if destination.panes.iter().any(|spec| spec.id == pane)
            || destination.station_ids().contains(&pane)
        {
            return Err(BlueprintViolation::DuplicatePaneSpec(pane));
        }
        let Some((spec, floating)) = self.take_floating_pane(pane) else {
            return Err(BlueprintViolation::MissingPaneSpec {
                space: self.id.clone(),
                pane,
            });
        };
        destination.insert_transferred_floating(spec, floating)
    }

    /// Remove a floating pane's spec and station together for a window move.
    /// Unlike [`Self::take_pane`], callers also receive the persisted float
    /// geometry rather than discarding it.
    pub fn take_floating_pane(&mut self, pane: PaneId) -> Option<(PaneSpec, FloatingPane)> {
        let float_index = self.floating.iter().position(|item| item.pane == pane)?;
        let spec_index = self.panes.iter().position(|item| item.id == pane)?;
        let floating = self.floating.remove(float_index);
        let spec = self.panes.remove(spec_index);
        self.normalize();
        Some((spec, floating))
    }

    pub fn take_pane(&mut self, pane: PaneId) -> Option<PaneSpec> {
        let index = self.panes.iter().position(|spec| spec.id == pane)?;
        if let Some(tree) = self.tiled.take() {
            self.tiled = tree.without_pane(pane);
        }
        self.floating.retain(|item| item.pane != pane);
        let spec = self.panes.remove(index);
        self.normalize();
        Some(spec)
    }

    pub fn insert_floating(
        &mut self,
        spec: PaneSpec,
        rect: RelativeRect,
    ) -> Result<(), BlueprintViolation> {
        if self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        let pane = spec.id;
        self.panes.push(spec);
        self.floating.push(FloatingPane {
            pane,
            rect,
            constraints: FloatSizeConstraints::default(),
            z: self
                .floating
                .iter()
                .map(|item| item.z)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
            pinned: false,
            visible: true,
        });
        Ok(())
    }

    pub(super) fn insert_transferred_floating(
        &mut self,
        spec: PaneSpec,
        mut floating: FloatingPane,
    ) -> Result<(), BlueprintViolation> {
        if spec.id != floating.pane || self.panes.iter().any(|pane| pane.id == spec.id) {
            return Err(BlueprintViolation::DuplicatePaneSpec(spec.id));
        }
        if self.station_ids().contains(&spec.id) {
            return Err(BlueprintViolation::DuplicatePaneStation(spec.id));
        }
        floating.z = self
            .floating
            .iter()
            .map(|item| item.z)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.panes.push(spec);
        self.floating.push(floating);
        Ok(())
    }

    pub(super) fn normalize_float_z(&mut self) {
        self.floating.sort_by_key(|item| (item.z, item.pane.0));
        for (index, float) in self.floating.iter_mut().enumerate() {
            float.z = index as u32 + 1;
        }
    }

    pub(super) fn station_ids(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        if let Some(tree) = &self.tiled {
            tree.collect_panes(&mut out);
        }
        out.extend(self.floating.iter().map(|item| item.pane));
        out
    }
}
