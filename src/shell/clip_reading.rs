// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::retained_clip::owner_worker::Pending;

type ClipResult = Result<crate::retained_clip::ClipView, crate::research_storage::SourceReadError>;
pub(super) struct PendingClipRead {
    pub(super) url: String,
    pub(super) id: eidetic::ManifestId,
    pub(super) root: std::path::PathBuf,
    pub(super) session: crate::panes::SessionId,
    pub(super) generation: uuid::Uuid,
    pub(super) identity: Arc<crate::identity::RootIdentity>,
    pub(super) bound: Arc<crate::custody_binding::BoundSealer>,
    pub(super) epoch: u64,
    pub(super) reader: Option<crate::research::ClipReader>,
    pub(super) response: Pending<ClipResult>,
}

fn reader_route_ready(
    pending: bool,
    live: bool,
    focused_target: bool,
    has_citation: bool,
    citation_current: bool,
    direct_retention_current: bool,
) -> bool {
    !pending
        && live
        && focused_target
        && if has_citation {
            citation_current
        } else {
            direct_retention_current
        }
}

impl Shell {
    pub(super) fn focused_retained_reader_ready(&self) -> bool {
        let snapshot = crate::observe::snapshot(&self.app);
        let Some(node) = snapshot.focused.as_ref().map(|focus| focus.member) else {
            return false;
        };
        let pending = self.pending_clip_reads.contains_key(&node)
            || self.ready_clip_reads.contains_key(&node);
        let live = snapshot
            .content
            .iter()
            .any(|(member, state)| *member == node && state == "live")
            && self
                .app
                .content
                .facts(node)
                .is_some_and(|facts| facts.engine == crate::retained_clip::ENGINE_ID);
        let Some((member, url)) = self.app.browser_command_target() else {
            return false;
        };
        let Ok(Some(crate::retained_clip::ClipRequest::Read(manifest))) =
            crate::retained_clip::request(&url)
        else {
            return false;
        };
        let reader = self.app.research_clip_readers.get(&node);
        let citation_current = reader.is_some_and(|reader| {
            reader.target.clip.manifest == manifest
                && self
                    .app
                    .research_clip_reader(node, &url, manifest)
                    .ok()
                    .flatten()
                    .is_some()
        });
        reader_route_ready(
            pending,
            live,
            member == node,
            reader.is_some(),
            citation_current,
            self.latest_retention_ready(),
        )
    }

    pub(super) fn request_clip_read(
        &mut self,
        node: uuid::Uuid,
        url: &str,
        id: eidetic::ManifestId,
    ) -> Result<(), String> {
        let root = self.app.clip_destination()?;
        let identity = self.app.identity.clone().ok_or(crate::identity::PENDING)?;
        let binding = self
            .custody_binding
            .as_ref()
            .ok_or(crate::identity::PENDING)?;
        if !Arc::ptr_eq(&identity, &binding.root) {
            return Err(crate::identity::PENDING.into());
        }
        let bound = binding.bound.clone();
        let epoch = bound.epoch().ok_or(crate::identity::PENDING)?;
        let reader = self.app.research_clip_reader(node, url, id)?;
        let limits = self.app.research.storage_limits()?;
        let response = self.clip_owner.read(
            root.clone(),
            id,
            crate::retained_clip::limits()?,
            bound.clone(),
            limits.clone(),
            limits.budget(),
            (&limits.clip_queue).into(),
        )?;
        self.pending_clip_reads.insert(
            node,
            PendingClipRead {
                url: url.into(),
                id,
                root,
                session: self.app.session_id,
                generation: self.app.source_capture_generation,
                identity,
                bound,
                epoch,
                reader,
                response,
            },
        );
        Ok(())
    }

    pub(super) fn drain_clip_reads(&mut self) {
        let ready: Vec<_> = self
            .pending_clip_reads
            .iter()
            .filter_map(|(node, pending)| match pending.response.try_recv() {
                Ok(result) => Some((*node, Ok(result.into_inner()))),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Some((*node, Err("Retained clip owner stopped".to_string())))
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            })
            .collect();
        for (node, result) in ready {
            let Some(pending) = self.pending_clip_reads.remove(&node) else {
                continue;
            };
            let current = self.app.session_id == pending.session
                && self.app.source_capture_generation == pending.generation
                && self
                    .app
                    .identity
                    .as_ref()
                    .is_some_and(|identity| Arc::ptr_eq(identity, &pending.identity))
                && self
                    .custody_binding
                    .as_ref()
                    .is_some_and(|binding| Arc::ptr_eq(&binding.bound, &pending.bound))
                && pending.bound.epoch() == Some(pending.epoch)
                && self.app.clip_destination().ok().as_ref() == Some(&pending.root)
                && self
                    .app
                    .graph_runtimes
                    .graph_containing_member(node)
                    .and_then(|graph| self.app.graph_runtimes.canvas(graph))
                    .is_some_and(|canvas| crate::browse::still_current(canvas, node, &pending.url));
            if !current {
                continue;
            }
            let document = result
                .and_then(|result| result.map_err(|error| error.to_string()))
                .and_then(|clip| {
                    if clip.id != pending.id {
                        return Err("Retained clip identity changed while reading".into());
                    }
                    match &pending.reader {
                        Some(reader) => self.app.finish_research_clip_document(
                            node,
                            &pending.url,
                            clip.id,
                            reader,
                            &clip,
                        ),
                        None => {
                            if self
                                .app
                                .research_clip_reader(node, &pending.url, clip.id)?
                                .is_some()
                            {
                                return Err(
                                    "The retained clip citation changed while reading.".into()
                                );
                            }
                            Ok(clip.document())
                        },
                    }
                });
            match document {
                Ok(document) => {
                    self.ready_clip_reads
                        .insert(node, (pending.url.clone(), document));
                    self.run_effects(vec![crate::action::Effect::SpawnContent {
                        node,
                        url: pending.url,
                    }]);
                },
                Err(error) => {
                    let effects = self
                        .app
                        .apply_update(crate::action::Update::ContentFailed { node, error });
                    self.run_effects(effects);
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::reader_route_ready;
    #[test]
    fn reader_wait_distinguishes_current_citation_and_fresh_direct_retention() {
        assert!(reader_route_ready(false, true, true, true, true, false));
        assert!(reader_route_ready(false, true, true, false, false, true));
        assert!(!reader_route_ready(true, true, true, true, true, true));
        assert!(!reader_route_ready(false, false, true, true, true, true));
        assert!(!reader_route_ready(false, true, false, true, true, true));
        assert!(!reader_route_ready(false, true, true, true, false, true));
        assert!(!reader_route_ready(false, true, true, false, false, false));
    }
}
