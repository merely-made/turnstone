// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::App;
use crate::action::Effect;

impl App {
    pub(crate) fn open_research_clip(
        &mut self,
        target: crate::research::ClipTarget,
    ) -> Vec<Effect> {
        if !self.research_clip_admissions().contains(&target.clip) {
            return self.saved_source_refusal(
                "The cited retained clip was withdrawn from this session.".into(),
            );
        }
        if let Err(error) = self.clip_destination() {
            return self.saved_source_refusal(error);
        }
        let pane = self
            .frisket
            .iter_leaves()
            .chain(
                self.lenses
                    .iter()
                    .flatten()
                    .flat_map(|layout| layout.iter_leaves()),
            )
            .find(|(_, content, graph)| {
                matches!(content, crate::panes::PaneContent::Orrery) && *graph == target.clip.graph
            })
            .map(|(pane, _, _)| pane);
        let Some(pane) = pane else {
            return self
                .saved_source_refusal("The cited retained clip has no open owning graph.".into());
        };
        self.reveal_pane(pane);
        self.focus = crate::surface::FocusTarget::Graph(pane);
        self.graph_pane_select_member(pane, target.clip.node);
        let hash_valid = eidetic::Hash::parse(&target.text_hash).is_ok_and(|hash| {
            hash.func() == eidetic::Hash::of(b"").func() && hash.to_string() == target.text_hash
        });
        if !hash_valid || target.selector.as_ref().is_some_and(|s| s.start >= s.end) {
            return self.saved_source_refusal("The retained clip citation is invalid.".into());
        }
        let Some(identity) = self.identity.clone() else {
            return self.saved_source_refusal(crate::identity::PENDING.into());
        };
        let custody_epoch = match self
            .research
            .clip_access()
            .and_then(|access| (access.epoch)().ok_or_else(|| crate::identity::PENDING.into()))
        {
            Ok(epoch) => epoch,
            Err(error) => return self.saved_source_refusal(error),
        };
        let node = target.clip.node;
        let url = crate::retained_clip::address(target.clip.manifest);
        self.research_clip_readers.insert(
            node,
            crate::research::ClipReader {
                target,
                generation: self.source_capture_generation,
                identity,
                custody_epoch,
            },
        );
        let mut effects = self.open_in_workbench();
        // Replacing the same content view must apply the new selected range.
        effects
            .retain(|effect| !matches!(effect, Effect::SpawnContent { node: n, .. } if *n == node));
        effects.push(Effect::SpawnContent { node, url });
        if let Some(pane) = self.active_pane {
            self.reveal_pane(pane);
            self.focus = crate::surface::FocusTarget::Pane(pane);
        }
        effects
    }
    /// Snapshot and validate the current citation before or after worker I/O.
    pub(crate) fn research_clip_reader(
        &self,
        node: uuid::Uuid,
        url: &str,
        id: eidetic::ManifestId,
    ) -> Result<Option<crate::research::ClipReader>, String> {
        let Some(reader) = self.research_clip_readers.get(&node) else {
            return Ok(None);
        };
        if reader.generation != self.source_capture_generation
            || !self
                .identity
                .as_ref()
                .is_some_and(|identity| std::sync::Arc::ptr_eq(identity, &reader.identity))
            || reader.target.clip.manifest != id
            || !self
                .research_clip_admissions()
                .contains(&reader.target.clip)
            || url != crate::retained_clip::address(id)
        {
            return Err("The retained clip citation is no longer admitted.".into());
        }
        let access = self.research.clip_access()?;
        if (access.epoch)() != Some(reader.custody_epoch) {
            return Err("Profile custody changed before reading the retained clip.".into());
        }
        Ok(Some(reader.clone()))
    }

    pub(crate) fn finish_research_clip_document(
        &self,
        node: uuid::Uuid,
        url: &str,
        id: eidetic::ManifestId,
        expected: &crate::research::ClipReader,
        clip: &crate::retained_clip::ClipView,
    ) -> Result<inker::EngineDocument, String> {
        let reader = self
            .research_clip_reader(node, url, id)?
            .ok_or("The retained clip citation is no longer admitted.")?;
        if reader.target != expected.target
            || reader.generation != expected.generation
            || reader.custody_epoch != expected.custody_epoch
            || !std::sync::Arc::ptr_eq(&reader.identity, &expected.identity)
        {
            return Err("The retained clip citation changed while reading.".into());
        }
        crate::research::clip_document(clip, &reader.target)
    }

    #[cfg(test)]
    pub(crate) fn research_clip_document(
        &self,
        node: uuid::Uuid,
        url: &str,
        id: eidetic::ManifestId,
    ) -> Result<Option<inker::EngineDocument>, String> {
        let Some(reader) = self.research_clip_reader(node, url, id)? else {
            return Ok(None);
        };
        let access = self.research.clip_access()?;
        let limits = self.research.storage_limits()?;
        let clip = self
            .research
            .clip_owner()?
            .read(
                access.root.clone(),
                id,
                crate::retained_clip::limits()?,
                access.sealer.clone(),
                limits.clone(),
                limits.budget(),
                (&limits.clip_queue).into(),
            )?
            .recv()
            .map_err(|_| "Retained clip owner stopped")?
            .into_inner()
            .map_err(|error| error.to_string())?;
        self.finish_research_clip_document(node, url, id, &reader, &clip)
            .map(Some)
    }
    pub(crate) fn invalidate_research_clip_readers(&mut self) -> Vec<Effect> {
        let admitted = self.research_clip_admissions();
        let custody_epoch = self
            .research
            .clip_access()
            .ok()
            .and_then(|access| (access.epoch)());
        let invalid: Vec<_> =
            self.research_clip_readers
                .iter()
                .filter_map(|(node, reader)| {
                    let still_current = self
                        .graph_runtimes
                        .get(reader.target.clip.graph)
                        .is_some_and(|runtime| {
                            crate::browse::still_current(
                                &runtime.canvas,
                                *node,
                                &crate::retained_clip::address(reader.target.clip.manifest),
                            )
                        });
                    (reader.generation != self.source_capture_generation
                        || custody_epoch != Some(reader.custody_epoch)
                        || !self.identity.as_ref().is_some_and(|identity| {
                            std::sync::Arc::ptr_eq(identity, &reader.identity)
                        })
                        || !admitted.contains(&reader.target.clip)
                        || !still_current)
                        .then_some(*node)
                })
                .collect();
        for node in &invalid {
            self.research_clip_readers.remove(node);
        }
        invalid
            .into_iter()
            .map(|node| Effect::CloseContent { node })
            .collect()
    }
}
