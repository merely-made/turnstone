// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

use inker::PageCaptureRequestId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CaptureTarget {
    pub session: uuid::Uuid,
    pub node: uuid::Uuid,
    pub document_generation: u64,
    pub surface: uuid::Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PendingCapture {
    pub request: PageCaptureRequestId,
    pub target: CaptureTarget,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum CaptureRefusal {
    UnknownOrDuplicate,
    StaleTarget,
    RequestIdExhausted,
}

#[derive(Default)]
pub(super) struct CaptureCorrelation {
    surfaces: HashMap<uuid::Uuid, (uuid::Uuid, u64)>,
    pending: HashMap<PageCaptureRequestId, PendingCapture>,
    next_request: u64,
}

impl CaptureCorrelation {
    pub(super) fn register_surface(&mut self, node: uuid::Uuid) {
        self.retire_surface(node);
        self.surfaces.insert(node, (uuid::Uuid::new_v4(), 0));
    }

    pub(super) fn remove_surface(&mut self, node: uuid::Uuid) {
        self.retire_surface(node);
    }

    pub(super) fn clear_surfaces(&mut self) {
        self.surfaces.clear();
        self.pending.clear();
    }

    fn retire_surface(&mut self, node: uuid::Uuid) {
        if let Some((surface, _)) = self.surfaces.remove(&node) {
            self.pending
                .retain(|_, pending| pending.target.surface != surface);
        }
    }

    pub(super) fn advance_document(&mut self, node: uuid::Uuid) {
        if let Some((surface, generation)) = self.surfaces.get_mut(&node) {
            if let Some(next) = generation.checked_add(1) {
                *generation = next;
            } else {
                let exhausted = *surface;
                *surface = uuid::Uuid::new_v4();
                *generation = 0;
                self.pending
                    .retain(|_, pending| pending.target.surface != exhausted);
            }
        }
    }

    pub(super) fn begin(
        &mut self,
        session: uuid::Uuid,
        node: uuid::Uuid,
    ) -> Result<PendingCapture, CaptureRefusal> {
        let Some((surface, generation)) = self.surfaces.get(&node).copied() else {
            return Err(CaptureRefusal::StaleTarget);
        };
        let value = self.next_request;
        self.next_request = self.next_request
            .checked_add(1)
            .ok_or(CaptureRefusal::RequestIdExhausted)?;
        let request = PageCaptureRequestId::new(value);
        let pending = PendingCapture {
            request,
            target: CaptureTarget {
                session,
                node,
                document_generation: generation,
                surface,
            },
        };
        self.pending.insert(request, pending);
        Ok(pending)
    }

    pub(super) fn finish(
        &mut self,
        session: uuid::Uuid,
        node: uuid::Uuid,
        request: PageCaptureRequestId,
    ) -> Result<PendingCapture, CaptureRefusal> {
        let Some((surface, generation)) = self.surfaces.get(&node).copied() else {
            return Err(CaptureRefusal::UnknownOrDuplicate);
        };
        let Some(pending) = self.pending.remove(&request) else {
            return Err(CaptureRefusal::UnknownOrDuplicate);
        };
        let live = CaptureTarget {
            session,
            node,
            document_generation: generation,
            surface,
        };
        (pending.target == live)
            .then_some(pending)
            .ok_or(CaptureRefusal::StaleTarget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_completion_is_single_use() {
        let session = uuid::Uuid::new_v4();
        let node = uuid::Uuid::new_v4();
        let mut registry = CaptureCorrelation::default();
        registry.register_surface(node);
        let pending = registry.begin(session, node).unwrap();
        assert_eq!(registry.finish(session, node, pending.request), Ok(pending));
        assert_eq!(
            registry.finish(session, node, pending.request),
            Err(CaptureRefusal::UnknownOrDuplicate)
        );
    }

    #[test]
    fn navigation_and_surface_replacement_refuse_old_completion() {
        let session = uuid::Uuid::new_v4();
        let node = uuid::Uuid::new_v4();
        let mut registry = CaptureCorrelation::default();
        registry.register_surface(node);
        let navigated = registry.begin(session, node).unwrap();
        registry.advance_document(node);
        assert_eq!(
            registry.finish(session, node, navigated.request),
            Err(CaptureRefusal::StaleTarget)
        );

        let replaced = registry.begin(session, node).unwrap();
        registry.register_surface(node);
        let current = registry.begin(session, node).unwrap();
        assert_eq!(
            registry.finish(session, node, replaced.request),
            Err(CaptureRefusal::UnknownOrDuplicate)
        );
        assert_eq!(registry.finish(session, node, current.request), Ok(current));
    }

    #[test]
    fn session_switch_refuses_completion() {
        let session = uuid::Uuid::new_v4();
        let node = uuid::Uuid::new_v4();
        let mut registry = CaptureCorrelation::default();
        registry.register_surface(node);
        let pending = registry.begin(session, node).unwrap();
        assert_eq!(
            registry.finish(uuid::Uuid::new_v4(), node, pending.request),
            Err(CaptureRefusal::StaleTarget)
        );
        let retry = registry.begin(session, node).unwrap();
        assert_ne!(retry.request, pending.request, "a refused id stays spent");
    }

    #[test]
    fn generation_exhaustion_rotates_surface_and_retires_pending() {
        let session = uuid::Uuid::new_v4();
        let node = uuid::Uuid::new_v4();
        let mut registry = CaptureCorrelation::default();
        registry.register_surface(node);
        registry.surfaces.get_mut(&node).unwrap().1 = u64::MAX;
        let pending = registry.begin(session, node).unwrap();
        registry.advance_document(node);
        assert_eq!(
            registry.finish(session, node, pending.request),
            Err(CaptureRefusal::UnknownOrDuplicate)
        );
    }

    #[test]
    fn admission_failure_spends_id_and_allocator_exhaustion_refuses() {
        let session = uuid::Uuid::new_v4();
        let node = uuid::Uuid::new_v4();
        let mut registry = CaptureCorrelation::default();
        registry.register_surface(node);
        let refused = registry.begin(session, node).unwrap();
        assert_eq!(registry.finish(session, node, refused.request), Ok(refused));
        let retry = registry.begin(session, node).unwrap();
        assert_ne!(retry.request, refused.request);

        registry.next_request = u64::MAX;
        assert_eq!(
            registry.begin(session, node),
            Err(CaptureRefusal::RequestIdExhausted)
        );
    }
}
