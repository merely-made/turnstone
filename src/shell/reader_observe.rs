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
    pub(super) display_scale: f32,
    pub(super) reading_preferences: tabard::ReadingPreferences,
    /// A deterministic group number for source-document identity within this
    /// one observation. It is assigned after sorting by surface id, so receipts
    /// can prove sharing without serializing process-local Arc addresses.
    pub(super) source_group: usize,
}

impl Shell {
    pub(super) fn retain_reading_positions(&mut self) {
        if self.app.reading.load_error.is_some() {return;}
        let panes:Vec<_>=self.app.frisket.iter_leaves().chain(self.app.lenses.iter().flatten().flat_map(|layout|layout.iter_leaves()))
            .filter(|(_,kind,_)|matches!(kind,crate::panes::PaneContent::Workbench)).map(|(pane,_,_)|pane).collect();
        let mut excerpts=std::collections::BTreeMap::new();
        for (node,content) in self.content_sessions.iter() {
            if let Some(controller)=content.document() {
                let session=controller.session().as_any_ref();
                let document=session.downcast_ref::<mere_document_lanes::ReaderDocumentSession>().map(|r|r.source_document())
                    .or_else(||session.downcast_ref::<mere_document_lanes::SmolwebDocumentSession>().map(|d|d.document().source_document()));
                if let Some(excerpt)=document.and_then(|doc|mere_document_lanes::reading_excerpt(&doc)) {excerpts.insert(*node,excerpt);}
            }
        }
        self.app.reading.excerpts=excerpts;
        let mut changed=false;
        for key in self.content_sessions.pool.keys().copied().collect::<Vec<_>>() {
            let super::content_pool::ContentKey::Reader {node,appearance}=key else {continue;};
            let target=panes.iter().find_map(|pane| {let target=crate::reading_view::ReadTarget{pane:*pane,member:node};
                (0..=self.app.lenses.len() as u64).any(|window|target.appearance(window)==appearance).then_some(target)});
            let Some(target)=target else {continue;};
            let Some(session)=self.content_sessions.pool.content(&key).and_then(pelt_core::PeltContent::document).map(|c|c.session().as_any_ref()) else {continue;};
            let anchor=session.downcast_ref::<mere_document_lanes::ReaderDocumentSession>().and_then(|r|r.reading_anchor())
                .or_else(||session.downcast_ref::<mere_document_lanes::SmolwebDocumentSession>().and_then(|d|d.document().reading_anchor()));
            if let Some(anchor)=anchor {
                changed|=self.app.retain_scope_anchor(target,&anchor);
                if let Some(trail)=self.app.reading.trails.get_mut(&target.pane) {
                    if trail.current().is_some_and(|visit|visit.member==node) && trail.current_anchor()!=Some(&anchor) {trail.anchor_current(anchor.clone());changed=true;}
                }
                if self.app.reading.anchors.get(&target)!=Some(&anchor) {self.app.reading.anchors.insert(target,anchor);changed=true;}
            }
        }
        if changed {if let Err(error)=self.app.reading.save(&self.app.session_dir()) {tracing::warn!(%error,"Reading positions were not saved");}}
    }

    pub(super) fn reader_appearance_observations(&self) -> Vec<ReaderAppearanceObservation> {
        let plan = self.surface_plan();
        let mut raw = plan
            .iter()
            .filter_map(|surface| {
                let SurfaceKind::Content(node) = surface.kind else {
                    return None;
                };
                let reader = self.content_sessions.appearance(node, surface.id)?
                    .document()?.session().as_any_ref()
                    .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()?;
                Some({
                    (
                        surface.id,
                        self.reader_appearance_role(node, surface.id, &plan),
                        surface.rect,
                        reader.viewport(),
                        reader.scroll_y(),
                        reader.display_scale(),
                        reader.reading_preferences(),
                        reader.source_document(),
                    )
                })
            })
            .collect::<Vec<_>>();
        raw.sort_by_key(|(id, ..)| id.0);

        let mut sources = Vec::new();
        raw.into_iter()
            .map(|(id, role, rect, viewport, scroll_y, display_scale, reading_preferences, source)| {
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
                    display_scale,
                    reading_preferences,
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
                    "id={:#018x} role={} source={} rect={:.0},{:.0} {:.0}x{:.0} viewport={}x{} scroll={} display_scale={} text_size={} column_em={}",
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
                    appearance.display_scale,
                    appearance.reading_preferences.text_size,
                    appearance.reading_preferences.column_em,
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
