// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{NodeSurfaces, Shell, window_input};
use inker::{DataTransfer, DataTransferItem, DragOperationSet, PhysicalPosition};
use winit::data_transfer::{DataTransferId, DataTransferSendBuilder, TypeHint};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, AsyncRequestSerial, DndAction};
use winit::window::WindowId;

struct Source {
    appearance: crate::surface::SurfaceId,
    slot: u64,
    node: uuid::Uuid,
    window: WindowId,
    generation: (uuid::Uuid, u64),
    position: PhysicalPosition,
}

#[derive(Default)]
pub(super) struct PageDrag {
    queued: Option<(Source, String)>,
    active: Option<(DataTransferId, Source)>,
    incoming: Option<IncomingFiles>,
}

struct IncomingFiles {
    id: DataTransferId,
    serial: AsyncRequestSerial,
    position: (f32, f32),
    paths: Option<Vec<std::path::PathBuf>>,
    dropped: bool,
}

impl PageDrag {
    fn has_source(&self, node: uuid::Uuid) -> bool {
        self.queued
            .as_ref()
            .is_some_and(|(source, _)| source.node == node)
            || self
                .active
                .as_ref()
                .is_some_and(|(_, source)| source.node == node)
    }

    fn take_completion(&mut self, window: WindowId, id: DataTransferId) -> Option<Source> {
        if self
            .active
            .as_ref()
            .is_some_and(|(expected, source)| *expected == id && source.window == window)
        {
            return self.active.take().map(|(_, source)| source);
        }
        None
    }

    fn take_matching(&mut self, matches: impl Fn(&Source) -> bool) -> Option<Source> {
        if self
            .queued
            .as_ref()
            .is_some_and(|(source, _)| matches(source))
        {
            return self.queued.take().map(|(source, _)| source);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|(_, source)| matches(source))
        {
            return self.active.take().map(|(_, source)| source);
        }
        None
    }
}

impl Shell {
    pub(super) fn queue_page_drag(
        &mut self,
        node: uuid::Uuid,
        position: PhysicalPosition,
        data: DataTransfer,
    ) {
        // Weld has already accepted this into its source loop. Either admit
        // the transfer here or send a terminal NONE back to this producer.
        if !cfg!(all(windows, feature = "weld"))
            || !self.content_sessions.get(&node).is_some_and(|content| {
                content.route().active_engine() == inker::routing::ENGINE_WELD_CHROMIUM
            })
        {
            if let Some(producer) = self.content_sessions.surface_mut(&node) {
                let _ = producer.finish_drag_source(position, DragOperationSet::NONE);
            }
            return;
        }
        if self.page_drag.has_source(node) {
            return;
        }
        let source = window_input::captured_mouse(&self.content_sessions.pool)
            .and_then(|id| self.content_sessions.pool.placement(&id))
            .and_then(|placement| {
                let hit = window_input::captured_content_hit(
                    &self.content_sessions.pool,
                    placement.window,
                    position,
                )?;
                if hit.kind != crate::surface::SurfaceKind::Content(node) {
                    return None;
                }
                let window = if placement.window == 0 {
                    self.window.as_ref().cloned()
                } else {
                    self.lens_windows
                        .iter()
                        .find(|(_, lens)| lens.ordinal as u64 + 1 == placement.window)
                        .map(|(_, lens)| lens.window.clone())
                }?;
                Some((window, hit.id, placement.window))
            });
        let has_link_or_file = data.items.iter().any(|item| match item {
            DataTransferItem::String { mime_type, .. } => {
                mime_type.eq_ignore_ascii_case("text/uri-list")
            },
            _ => true,
        });
        let text = data.items.into_iter().find_map(|item| match item {
            DataTransferItem::String { mime_type, data }
                if mime_type == "text/plain" && !data.is_empty() =>
            {
                Some(data)
            },
            _ => None,
        });
        if !has_link_or_file
            && data.allowed_operations.contains(DragOperationSet::COPY)
            && self.page_drag.queued.is_none()
            && self.page_drag.active.is_none()
            && let (Some((window, appearance, slot)), Some(text), Some(generation)) =
                (source, text, self.page_captures.source_generation(node))
        {
            self.page_drag.queued = Some((
                Source {
                    node,
                    appearance,
                    slot,
                    position,
                    window: window.id(),
                    generation,
                },
                text,
            ));
            window.request_redraw();
        } else if let Some(producer) = self.content_sessions.surface_mut(&node) {
            let _ = producer.finish_drag_source(position, DragOperationSet::NONE);
        }
    }

    /// Admit the drag only during the source window's RedrawRequested.
    /// Winit beta.4 enters the synchronous Win32 OLE loop when this native
    /// window callback returns. AboutToWait then observes the outer pump.
    pub(super) fn start_pending_page_drag(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window: WindowId,
    ) -> bool {
        let Some((source, _)) = &self.page_drag.queued else {
            return false;
        };
        if source.window != window {
            return false;
        }
        let (source, text) = self.page_drag.queued.take().unwrap();
        if window_input::captured_mouse(&self.content_sessions.pool) != Some(source.appearance)
            || self.page_captures.source_generation(source.node) != Some(source.generation)
        {
            self.finish_page_drag(source, DragOperationSet::NONE);
            return false;
        }
        let offer = DataTransferSendBuilder::new(text)
            .with_type(TypeHint::Plaintext, |text, _| Some(text.clone()));
        match event_loop.start_drag(window, offer.build(), &[DndAction::Copy], None) {
            Ok(id) => {
                tracing::info!(?id, "started native page text drag");
                self.page_drag.active = Some((id, source));
                true
            },
            Err(error) => {
                tracing::warn!(%error, "native page text drag refused");
                self.finish_page_drag(source, DragOperationSet::NONE);
                false
            },
        }
    }

    pub(super) fn settle_missing_page_drag_completion(&mut self) {
        // On Win32, AboutToWait runs only on the outer message pump. A
        // successful or canceled DoDragDrop already delivered completion
        // before this point. beta.4 can return an error without an event.
        if let Some((id, source)) = self.page_drag.active.take() {
            tracing::warn!(?id, "native page drag ended without completion");
            self.finish_page_drag(source, DragOperationSet::NONE);
        }
    }

    pub(super) fn retire_page_drag_for_document(&mut self, node: uuid::Uuid) {
        self.retire_page_drag(|source| source.node == node);
    }

    pub(super) fn retire_all_page_drags(&mut self) {
        self.retire_page_drag(|_| true);
    }

    fn retire_page_drag(&mut self, matches: impl Fn(&Source) -> bool) {
        if let Some(source) = self.page_drag.take_matching(matches) {
            self.finish_page_drag(source, DragOperationSet::NONE);
        }
    }

    fn finish_page_drag(&mut self, source: Source, operation: DragOperationSet) {
        if self.page_captures.source_generation(source.node) == Some(source.generation)
            && let Some(producer) = self.content_sessions.surface_mut(&source.node)
        {
            if let Err(error) = producer.finish_drag_source(source.position, operation) {
                tracing::warn!(%error, "page text drag completion failed");
            }
        }
        if window_input::captured_mouse(&self.content_sessions.pool) == Some(source.appearance)
            && self.page_captures.source_generation(source.node) == Some(source.generation)
        {
            self.window_buttons.remove(&source.slot);
            if self.pointer_capture == Some(crate::surface::SurfaceKind::Content(source.node)) {
                self.pointer_capture = None;
            }
            self.content_input(
                source.appearance,
                pelt_core::PeltInput::Session(inker::SessionInput::Cancel),
            );
            if source.slot == 0 {
                self.surface_pointer_buttons = inker::PointerButtons::NONE;
            }
        }
        tracing::info!(?operation, "finished native page text drag");
    }

    pub(super) fn handle_native_drag(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window: WindowId,
        event: &WindowEvent,
    ) -> bool {
        match event {
            WindowEvent::OutgoingDragDropped { id, .. }
            | WindowEvent::OutgoingDragCanceled { id } => {
                if let Some(source) = self.page_drag.take_completion(window, *id) {
                    let operation = if matches!(
                        event,
                        WindowEvent::OutgoingDragDropped {
                            action: Some(DndAction::Copy),
                            ..
                        }
                    ) {
                        DragOperationSet::COPY
                    } else {
                        DragOperationSet::NONE
                    };
                    self.finish_page_drag(source, operation);
                }
            },
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                self.retire_page_drag(|source| source.window == window);
                if self.window.as_ref().is_some_and(|main| main.id() == window) {
                    self.page_drag.incoming = None;
                    self.cancel_host_file_drag();
                }
                return false;
            },
            _ if self.window.as_ref().map(|w| w.id()) != Some(window) => return false,
            WindowEvent::DragEntered { id, position } => {
                self.cancel_host_file_drag();
                self.page_drag.incoming = None;
                if event_loop
                    .data_transfer(*id)
                    .is_ok_and(|offer| offer.has_type(&TypeHint::UriList))
                    && event_loop
                        .set_valid_dnd_actions(*id, &[DndAction::Copy])
                        .is_ok()
                    && let Ok(serial) = event_loop.fetch_data_transfer(*id, &TypeHint::UriList)
                {
                    self.page_drag.incoming = Some(IncomingFiles {
                        id: *id,
                        serial,
                        position: position.map_or(self.cursor, |p| (p.x as f32, p.y as f32)),
                        paths: None,
                        dropped: false,
                    });
                }
            },
            WindowEvent::DragPosition { id, position, .. } => {
                if let Some(incoming) = self.page_drag.incoming.as_mut().filter(|d| d.id == *id) {
                    let (x, y) = (position.x as f32, position.y as f32);
                    incoming.position = (x, y);
                    self.update_host_file_drag(x, y);
                }
            },
            WindowEvent::DragLeft { id } => {
                if self
                    .page_drag
                    .incoming
                    .as_ref()
                    .is_some_and(|d| d.id == *id)
                {
                    self.page_drag.incoming = None;
                    self.cancel_host_file_drag();
                }
            },
            WindowEvent::DataTransferReceived { id, serial, value } => {
                let Some(incoming) = self
                    .page_drag
                    .incoming
                    .as_mut()
                    .filter(|d| d.id == *id && d.serial == *serial)
                else {
                    return false;
                };
                let uris = match value.try_as_uris() {
                    Ok(uris) => uris,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Deadlock
                        ) =>
                    {
                        return true;
                    },
                    Err(error) => {
                        tracing::debug!(%error, "could not read file drag");
                        self.page_drag.incoming = None;
                        self.cancel_host_file_drag();
                        return true;
                    },
                };
                let paths: Vec<_> = uris
                    .iter()
                    .filter_map(|uri| url::Url::parse(uri).ok()?.to_file_path().ok())
                    .collect();
                let (x, y) = incoming.position;
                incoming.paths = Some(paths.clone());
                if !incoming.dropped {
                    self.hover_host_files(x, y, &paths);
                }
            },
            WindowEvent::DragDropped { id, .. } => {
                if let Some(incoming) = self.page_drag.incoming.as_mut().filter(|d| d.id == *id) {
                    incoming.dropped = true;
                }
            },
            _ => return false,
        }
        if self
            .page_drag
            .incoming
            .as_ref()
            .is_some_and(|d| d.dropped && d.paths.is_some())
        {
            let incoming = self.page_drag.incoming.take().unwrap();
            self.drop_host_files(
                incoming.position.0,
                incoming.position.1,
                &incoming.paths.unwrap_or_default(),
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(window: WindowId) -> Source {
        Source {
            appearance: crate::surface::SurfaceId(1),
            slot: 0,
            node: uuid::Uuid::new_v4(),
            window,
            generation: (uuid::Uuid::new_v4(), 0),
            position: PhysicalPosition { x: 1.0, y: 2.0 },
        }
    }

    #[test]
    fn retiring_a_source_allows_another_drag_and_ignores_late_completion() {
        // Synthetic IDs exercise correlation only. They are never sent to winit.
        let window = WindowId::from_raw(1);
        let old_id = DataTransferId::from_raw(1);
        let new_id = DataTransferId::from_raw(2);
        let to_retire = source(window);
        let node = to_retire.node;
        let mut drag = PageDrag {
            active: Some((old_id, to_retire)),
            ..Default::default()
        };

        assert!(drag.take_matching(|source| source.node != node).is_none());
        assert_eq!(
            drag.take_matching(|source| source.node == node)
                .unwrap()
                .node,
            node
        );
        assert!(!drag.has_source(node));

        drag.active = Some((new_id, source(window)));
        assert!(drag.take_completion(window, old_id).is_none());
        assert!(
            drag.take_completion(WindowId::from_raw(2), new_id)
                .is_none()
        );
        assert!(drag.take_completion(window, new_id).is_some());
        assert!(drag.take_completion(window, new_id).is_none());
    }

    #[test]
    fn retiring_a_window_also_discards_a_drag_waiting_to_start() {
        let window = WindowId::from_raw(1);
        let queued = source(window);
        let node = queued.node;
        let mut drag = PageDrag {
            queued: Some((queued, "selection".into())),
            ..Default::default()
        };
        assert!(drag.has_source(node));
        assert!(
            drag.take_matching(|source| source.window == window)
                .is_some()
        );
        assert!(!drag.has_source(node));
    }
}
