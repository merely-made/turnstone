// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Read-only observations of Reader's live surface appearances.
//!
//! This lives with the shell because the shell owns both the appearance
//! registry and the surface plan. App-level observation deliberately has no
//! viewport or renderer-session handles to inspect.

use std::sync::Arc;

use crate::surface::{Rect, SurfaceId, SurfaceKind};

use super::Shell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReaderAppearanceRole {
    Inset,
    Workbench,
    Tile,
    Unknown,
}

impl ReaderAppearanceRole {
    pub(super) fn parse(value: &str) -> Option<Self> {
        match value {
            "inset" => Some(Self::Inset),
            "workbench" => Some(Self::Workbench),
            "tile" => Some(Self::Tile),
            _ => None,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Inset => "inset",
            Self::Workbench => "workbench",
            Self::Tile => "tile",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReaderAppearanceObservation {
    pub(super) id: SurfaceId,
    pub(super) role: ReaderAppearanceRole,
    pub(super) rect: Rect,
    pub(super) viewport: (u32, u32),
    pub(super) scroll_y: f32,
    /// A deterministic group number for source-document identity within this
    /// one observation. It is assigned after sorting by surface id, so receipts
    /// can prove sharing without serializing process-local Arc addresses.
    pub(super) source_group: usize,
}

impl Shell {
    pub(super) fn reader_appearance_observations(&self) -> Vec<ReaderAppearanceObservation> {
        let plan = self.surface_plan();
        let mut raw = plan
            .iter()
            .filter_map(|surface| {
                let SurfaceKind::Content(node) = surface.kind else {
                    return None;
                };
                let (known, reader) = self.reader_appearances.get(&surface.id)?;
                (*known == node).then(|| {
                    (
                        surface.id,
                        self.reader_appearance_role(node, surface.id, &plan),
                        surface.rect,
                        reader.viewport(),
                        reader.scroll_y(),
                        reader.source_document(),
                    )
                })
            })
            .collect::<Vec<_>>();
        raw.sort_by_key(|(id, ..)| id.0);

        let mut sources = Vec::new();
        raw.into_iter()
            .map(|(id, role, rect, viewport, scroll_y, source)| {
                let source_group = sources
                    .iter()
                    .position(|known| Arc::ptr_eq(known, &source))
                    .unwrap_or_else(|| {
                        sources.push(source);
                        sources.len() - 1
                    });
                ReaderAppearanceObservation {
                    id,
                    role,
                    rect,
                    viewport,
                    scroll_y,
                    source_group,
                }
            })
            .collect()
    }

    fn reader_appearance_role(
        &self,
        node: uuid::Uuid,
        id: SurfaceId,
        plan: &[crate::surface::Surface],
    ) -> ReaderAppearanceRole {
        for surface in plan {
            if let SurfaceKind::Graph(pane) = surface.kind
                && SurfaceId::content_appearance(node, 0x3000_0000_0000_0000 | pane.0) == id
            {
                return ReaderAppearanceRole::Inset;
            }
            if let SurfaceKind::Pane(pane) = surface.kind
                && matches!(
                    self.app.pane_content(pane),
                    Some(crate::panes::PaneContent::Workbench)
                )
                && SurfaceId::content_appearance(
                    node,
                    crate::surface::appearance::workbench_appearance_role(pane, node),
                ) == id
            {
                return ReaderAppearanceRole::Workbench;
            }
        }
        for (pane, content, _) in self.app.frisket.iter_leaves() {
            if matches!(content, crate::panes::PaneContent::Tile(member) if *member == node)
                && SurfaceId::content_appearance(node, 0x1000_0000_0000_0000 | pane.0) == id
            {
                return ReaderAppearanceRole::Tile;
            }
        }
        ReaderAppearanceRole::Unknown
    }

    pub(super) fn scroll_reader_appearance(
        &mut self,
        role: ReaderAppearanceRole,
        dy: f32,
    ) -> Result<(), String> {
        let appearances = self.reader_appearance_observations();
        let matching = appearances
            .iter()
            .filter(|appearance| appearance.role == role)
            .collect::<Vec<_>>();
        let [appearance] = matching.as_slice() else {
            return Err(format!(
                "scroll-reader {}: expected one live appearance, observed {}",
                role.label(),
                Self::describe_reader_appearances(&appearances)
            ));
        };
        self.deliver_wheel(
            appearance.rect.x + appearance.rect.w / 2.0,
            appearance.rect.y + appearance.rect.h / 2.0,
            0.0,
            dy,
        );
        Ok(())
    }

    pub(super) fn describe_reader_appearances(
        appearances: &[ReaderAppearanceObservation],
    ) -> String {
        appearances
            .iter()
            .map(|appearance| {
                format!(
                    "id={:#018x} role={} source={} rect={:.0},{:.0} {:.0}x{:.0} viewport={}x{} scroll={}",
                    appearance.id.0,
                    appearance.role.label(),
                    appearance.source_group,
                    appearance.rect.x,
                    appearance.rect.y,
                    appearance.rect.w,
                    appearance.rect.h,
                    appearance.viewport.0,
                    appearance.viewport.1,
                    appearance.scroll_y,
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
