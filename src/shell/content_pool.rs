// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Host identities and Reader appearance custody over the shared Pelt pool.
use crate::surface::{Surface, SurfaceId};
use netrender::Scene;
use pelt_core::{PeltContent, PeltControllerPool};
use std::sync::Arc;

// Held fallback for reading symbols missing from native prose families.
const READING_SYMBOL_FONT: &[u8] = include_bytes!("../../assets/NotoSansMath-Regular.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ContentKey {
    Node(uuid::Uuid),
    Reader {
        node: uuid::Uuid,
        appearance: SurfaceId,
    },
}

pub(super) struct HostPlacement {
    /// The surface in the window's physical plan.
    pub surface: Surface,
    pub window: u64,
    pub focused: bool,
    /// The window's layout scale. The pool keeps `rect` in layout pixels
    /// (`surface.rect / scale_factor`): documents are laid out and rasterized
    /// at that size and scale, and surface producers convert to physical
    /// pixels at their own boundary.
    pub scale_factor: f32,
    pub reading_preferences: tabard::ReadingPreferences,
    pub reading_anchor: Option<tabard::ReadingAnchor>,
    /// The app theme's mode, under which themed smolweb and Reader
    /// appearances derive their colours.
    pub color_mode: mere_document_lanes::ModeProfile,
}

impl HostPlacement {
    pub(super) fn for_focus(
        surface: Surface,
        window: u64,
        active: bool,
        focus: crate::surface::FocusTarget,
        scale: f64,
    ) -> Self {
        Self::at(
            surface,
            window,
            active
                && matches!(focus, crate::surface::FocusTarget::Content { node, appearance }
                if surface.kind == crate::surface::SurfaceKind::Content(node) && surface.id == appearance),
            scale,
        )
    }

    pub(super) fn at(surface: Surface, window: u64, focused: bool, scale: f64) -> Self {
        Self {
            surface,
            window,
            focused,
            scale_factor: crate::ui::clean_scale(scale as f32),
            reading_preferences: Default::default(),
            reading_anchor: None,
            color_mode: mere_document_lanes::ModeProfile::LIGHT,
        }
    }
}

#[derive(Default)]
pub(crate) struct NodeDocuments {
    admissions: std::collections::HashMap<uuid::Uuid, uuid::Uuid>,
    pub(super) cue_cursors: std::collections::HashMap<uuid::Uuid, inker::CursorShape>,
    pub(super) pool: PeltControllerPool<ContentKey, SurfaceId, Scene>,
}

impl NodeDocuments {
    pub(super) fn admission(&self, node: uuid::Uuid) -> Option<uuid::Uuid> {
        self.admissions.get(&node).copied()
    }

    pub fn get(&self, node: &uuid::Uuid) -> Option<&PeltContent<Scene>> {
        self.pool.content(&ContentKey::Node(*node))
    }
    pub fn get_mut(&mut self, node: &uuid::Uuid) -> Option<&mut PeltContent<Scene>> {
        self.pool.content_mut(&ContentKey::Node(*node))
    }
    pub fn contains_key(&self, node: &uuid::Uuid) -> bool {
        self.get(node).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&uuid::Uuid, &PeltContent<Scene>)> {
        self.pool.iter().filter_map(|(key, content)| match key {
            ContentKey::Node(node) => Some((node, content)),
            _ => None,
        })
    }
    pub fn values(&self) -> impl Iterator<Item = &PeltContent<Scene>> {
        self.iter().map(|(_, content)| content)
    }
    pub fn insert(
        &mut self,
        node: uuid::Uuid,
        content: PeltContent<Scene>,
    ) -> Option<PeltContent<Scene>> {
        self.clear_appearances(node);
        self.cue_cursors.remove(&node);
        self.admissions.insert(node, uuid::Uuid::new_v4());
        let mut old = self.pool.insert(ContentKey::Node(node), content);
        if let Some(surface) = old.as_mut().and_then(PeltContent::cue_surface_mut) {
            let _ = surface.retire();
        }
        old
    }
    pub fn remove(&mut self, node: &uuid::Uuid) -> Option<PeltContent<Scene>> {
        self.clear_appearances(*node);
        self.admissions.remove(node);
        self.cue_cursors.remove(node);
        let mut old = self.pool.remove_content(&ContentKey::Node(*node));
        if let Some(surface) = old.as_mut().and_then(PeltContent::cue_surface_mut) {
            let _ = surface.retire();
        }
        old
    }
    pub fn clear(&mut self) {
        self.admissions.clear();
        self.cue_cursors.clear();
        let keys: Vec<_> = self.pool.keys().copied().collect();
        for key in keys {
            if let Some(mut content) = self.pool.remove_content(&key)
                && let Some(surface) = content.cue_surface_mut()
            {
                let _ = surface.retire();
            }
        }
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&uuid::Uuid, &PeltContent<Scene>) -> bool) {
        let removed: Vec<_> = self
            .iter()
            .filter_map(|(node, content)| (!keep(node, content)).then_some(*node))
            .collect();
        for node in removed {
            self.remove(&node);
        }
    }
    pub(super) fn retain_appearances(
        &mut self,
        mut keep: impl FnMut(uuid::Uuid, SurfaceId) -> bool,
    ) {
        let removed: Vec<_> = self
            .pool
            .keys()
            .filter_map(|key| match *key {
                ContentKey::Reader { node, appearance } if !keep(node, appearance) => Some(*key),
                _ => None,
            })
            .collect();
        for key in removed {
            if let Some(mut content) = self.pool.remove_content(&key)
                && let Some(surface) = content.cue_surface_mut()
            {
                let _ = surface.retire();
            }
        }
    }
    pub(super) fn clear_appearances(&mut self, node: uuid::Uuid) {
        self.retain_appearances(|known, _| known != node);
    }
    pub(super) fn appearance(
        &self,
        node: uuid::Uuid,
        appearance: SurfaceId,
    ) -> Option<&PeltContent<Scene>> {
        self.pool
            .content(&ContentKey::Reader { node, appearance })
            .or_else(|| self.get(&node))
    }
    pub(super) fn sync_placements(
        &mut self,
        placements: Vec<HostPlacement>,
        engines: Arc<inker::SessionRegistry<Scene>>,
        surface_engines: Arc<inker::SurfaceEngineRegistry>,
        epoch: std::time::Instant,
    ) {
        let mut arranged = Vec::new();
        for host in placements {
            let crate::surface::SurfaceKind::Content(node) = host.surface.kind else {
                continue;
            };
            let Some(canonical) = self.get(&node) else {
                continue;
            };
            let mut key = ContentKey::Node(node);
            if let Some(controller) = canonical.document() {
                let reader = controller
                    .session()
                    .as_any_ref()
                    .downcast_ref::<mere_document_lanes::ReaderDocumentSession>();
                let portable = controller
                    .session()
                    .as_any_ref()
                    .downcast_ref::<mere_document_lanes::SmolwebDocumentSession>();
                let source = reader
                    .map(|reader| reader.source_document())
                    .or_else(|| portable.map(|doc| doc.document().source_document()));
                if let Some(source) = source {
                    key = ContentKey::Reader {
                        node,
                        appearance: host.surface.id,
                    };
                    let current = self.pool.content(&key).and_then(PeltContent::document).is_some_and(|retained| {
                        let session=retained.session().as_any_ref();
                        let retained_source = session.downcast_ref::<mere_document_lanes::ReaderDocumentSession>().map(|r|r.source_document())
                            .or_else(|| session.downcast_ref::<mere_document_lanes::SmolwebDocumentSession>().map(|d|d.document().source_document()));
                        retained_source.is_some_and(|retained|Arc::ptr_eq(&source,&retained))
                    });
                    if !current {
                        let session: Box<dyn inker::DocumentSession<Scene>> =
                            if let Some(reader) = reader {
                                let mut appearance = reader.new_appearance((1, 1));
                                if let Some(anchor) = host.reading_anchor.clone() {
                                    appearance.restore_reading_anchor(anchor);
                                }
                                appearance.set_fonts(vec![READING_SYMBOL_FONT.to_vec()]);
                                appearance.set_font_family("serif, sans-serif, \"Noto Sans Math\"");
                                Box::new(appearance)
                            } else {
                                let mut appearance = portable.unwrap().new_appearance((1, 1));
                                if let Some(anchor) = host.reading_anchor.clone() {
                                    appearance.document_mut().restore_reading_anchor(anchor);
                                }
                                Box::new(appearance)
                            };
                        let config = pelt_core::PeltControllerConfig::from_request(
                            controller.engine_id(),
                            controller.request().clone(),
                        )
                        .with_host_history();
                        let route = canonical.route().clone();
                        let controller = pelt_core::PeltController::from_session_shared(
                            engines.clone(),
                            surface_engines.clone(),
                            config,
                            super::ShellClock(epoch),
                            session,
                        );
                        self.pool
                            .insert(key, PeltContent::from_controller(controller, route));
                    }
                }
            }
            let stale = self
                .pool
                .content(&key)
                .and_then(PeltContent::document)
                .is_some_and(|controller| {
                    let session = controller.session().as_any_ref();
                    session
                        .downcast_ref::<mere_document_lanes::ReaderDocumentSession>()
                        .is_some_and(|reader| {
                            reader.reading_preferences() != host.reading_preferences
                                || reader.color_mode() != host.color_mode
                        })
                        || session
                            .downcast_ref::<mere_document_lanes::SmolwebDocumentSession>()
                            .is_some_and(|document| {
                                let document = document.document();
                                document.reading_preferences() != host.reading_preferences
                                    || document.color_mode() != host.color_mode
                            })
                });
            if stale {
                if let Some(controller) = self
                    .pool
                    .content_mut(&key)
                    .and_then(PeltContent::document_mut)
                {
                    let session = controller.session_mut();
                    if let Some(reader) = session
                        .as_any()
                        .downcast_mut::<mere_document_lanes::ReaderDocumentSession>()
                    {
                        let _ = reader.set_reading_preferences(host.reading_preferences);
                        reader.set_color_mode(host.color_mode);
                    } else if let Some(document) = session
                        .as_any()
                        .downcast_mut::<mere_document_lanes::SmolwebDocumentSession>(
                    ) {
                        let document = document.document_mut();
                        let _ = document.set_reading_preferences(host.reading_preferences);
                        document.set_color_mode(host.color_mode);
                    }
                }
            }
            let (r, scale) = (host.surface.rect, host.scale_factor);
            arranged.push((
                host.surface.id,
                pelt_core::PeltPlacement {
                    content: key,
                    window: host.window,
                    rect: pelt_core::WorkspaceRect::new(
                        r.x / scale,
                        r.y / scale,
                        r.w / scale,
                        r.h / scale,
                    ),
                    visible: true,
                    focused: host.focused,
                    scale_factor: host.scale_factor,
                },
            ));
        }
        self.pool.set_placements(arranged);
    }
}

/// Request admitted pending windows before advancing a due document turn too:
/// its terminal pump may change the painted state while retiring pending work.
pub(super) fn pump_document_turn<T>(
    host: &mut T,
    mut request_redraws: impl FnMut(&T),
    pump: impl FnOnce(&mut T),
) {
    request_redraws(host);
    pump(host);
    request_redraws(host);
}

/// Event-loop document work shares the existing configured polling cadence;
/// pending work schedules ticks independently of window paint requests.
pub(super) struct DocumentClock {
    clock: super::surface_poll::SurfacePollClock,
    pending: bool,
    timers: Vec<(pelt_core::PeltDocumentWork<ContentKey>, std::time::Instant)>,
}
impl DocumentClock {
    pub(super) fn new(interval: std::time::Duration) -> Self {
        Self {
            clock: super::surface_poll::SurfacePollClock::new(interval),
            pending: false,
            timers: Vec::new(),
        }
    }
    pub(super) fn observe(&mut self, pending: bool) {
        self.pending = pending;
    }
    pub(super) fn observe_work(
        &mut self,
        pending: bool,
        work: impl IntoIterator<Item = pelt_core::PeltDocumentWork<ContentKey>>,
        pump_started: std::time::Instant,
    ) {
        let work: Vec<_> = work.into_iter().collect();
        self.observe(
            pending
                || work
                    .iter()
                    .any(|work| work.work.microtasks || work.work.external),
        );
        self.timers = work
            .into_iter()
            .filter_map(|observation| {
                let delay = observation.work.timer?;
                let deadline = inker::session_engine::SessionPendingWork::timer(delay)
                    .next_wake(pump_started)?;
                Some((observation, deadline))
            })
            .collect();
    }
    pub(super) fn due_work(
        &self,
        now: std::time::Instant,
    ) -> Vec<pelt_core::PeltDocumentWork<ContentKey>> {
        self.timers
            .iter()
            .filter(|(_, deadline)| *deadline <= now)
            .map(|(work, _)| work.clone())
            .collect()
    }
    pub(super) fn deadline(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        self.clock
            .deadline(self.pending, now)
            .into_iter()
            .chain(self.timers.iter().map(|(_, deadline)| *deadline))
            .min()
    }
    pub(super) fn poll_due(&mut self, now: std::time::Instant) -> bool {
        self.clock.poll_due(self.pending, now)
            || self.timers.iter().any(|(_, deadline)| *deadline <= now)
    }
}

#[cfg(test)]
#[path = "content_pool/tests.rs"]
mod tests;
