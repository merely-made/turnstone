// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Captured Inspector offers over existing source and Knot endpoint owners.
use super::{NodeSessions, Shell};
use crate::action::Action;
use crate::app::research_note_arms::NoteChoice;
use crate::knot_authoring::{KnotClipHandle, KnotDocumentSession};
use crate::panes::{PaneContext, PaneId};

#[derive(Clone)]
pub(super) struct ClipOffer {
    id: uuid::Uuid,
    pane: PaneId,
    dom: uuid::Uuid,
    context: PaneContext,
    context_admission: uuid::Uuid,
    generation: uuid::Uuid,
    admission: uuid::Uuid,
    note: Option<NoteChoice>,
    clip: inker::DocumentClip,
    target: KnotClipHandle,
}

impl ClipOffer {
    fn same_source(&self, other: &Self) -> bool {
        self.pane == other.pane
            && self.dom == other.dom
            && self.context == other.context
            && self.context_admission == other.context_admission
            && self.generation == other.generation
            && self.admission == other.admission
            && self.note == other.note
            && self.clip == other.clip
    }
}

impl Shell {
    pub(super) fn create_research_note(&mut self, generation: uuid::Uuid) {
        if generation != self.app.source_capture_generation {
            return;
        }
        let result = (|| {
            let root = self
                .knot_notes_root
                .as_ref()
                .ok_or("This Knot endpoint does not offer local note creation.")?;
            let path = root.join(format!("research-{}.djot", uuid::Uuid::new_v4()));
            let mut note =
                knot_document::KnotDocumentSession::scratch("research:new", "# Research note\n\n");
            note.apply(knot_document::KnotDocumentIntentV1::SaveAs(path.clone()))
                .map_err(|e| format!("Knot could not create the note: {e:?}"))?;
            url::Url::from_file_path(path)
                .map(|u| u.to_string())
                .map_err(|_| "The research note path is not an absolute file address.".to_string())
        })();
        match result {
            Ok(address) => self.act(Action::ResearchNoteCreated {
                generation,
                address,
            }),
            Err(reason) => self.research_note_refusal(reason),
        }
    }

    pub(super) fn choose_research_note(&mut self, choice: NoteChoice) {
        if self.app.note_choice(choice.member).as_ref() != Some(&choice) {
            self.research_note_refusal(
                "The note choice belongs to a stale session or member.".into(),
            );
            return;
        }
        let admitted = self
            .content_sessions
            .session(&choice.member)
            .and_then(|s| s.as_any_ref().downcast_ref::<KnotDocumentSession>())
            .ok_or("Open the local Knot authoring document before choosing it.")
            .and_then(|s| {
                s.research_clip_handle()
                    .map_err(|_| "Save or reload the local note before choosing it.")
            });
        match admitted {
            Ok(_) => self.act(Action::CommitResearchNote { choice }),
            Err(reason) => self.research_note_refusal(reason.into()),
        }
    }

    fn research_note_refusal(&mut self, reason: String) {
        self.app
            .note(crate::observe::AppEvent::AffordanceUnavailable {
                what: "research-note",
                target: reason,
            });
        self.request_redraw();
    }

    fn proposed_clip_offer(&self, pane: PaneId, dom: uuid::Uuid) -> Result<ClipOffer, String> {
        let context = self
            .app
            .follower_context(pane)
            .ok_or("The Inspector has no document context.")?;
        let member = context
            .member
            .ok_or("The Inspector has no displayed document.")?;
        let admission = self
            .content_sessions
            .admission(member)
            .ok_or("The displayed document is unavailable.")?;
        let note = self.app.research_note()?;
        let target = if let Some(note) = &note {
            if self.app.note_choice(note.member).as_ref() != Some(note) {
                return Err("The research note binding changed.".into());
            }
            self.content_sessions
                .session(&note.member)
                .and_then(|s| s.as_any_ref().downcast_ref::<KnotDocumentSession>())
                .ok_or("Open the chosen research note before clipping.")?
                .research_clip_handle()?
        } else {
            self.knot_clip
                .clone()
                .ok_or("Create or choose a research note first.")?
        };
        let clip = if let Some(reader) = self.app.saved_readers.get(&member) {
            if self
                .app
                .note_choice(member)
                .is_none_or(|source| source.address != reader.url)
                || !self
                    .app
                    .content
                    .facts(member)
                    .is_some_and(|facts| facts.engine == crate::saved_sources::ENGINE_ID)
                || reader.request.is_some()
                || reader.error.is_some()
                || !self.app.saved_source_admitted(&reader.target.key)
            {
                return Err("The displayed saved passage is unavailable or withdrawn.".into());
            }
            crate::saved_sources::clip(
                reader
                    .content
                    .as_ref()
                    .ok_or("The saved passage is not ready.")?,
                &reader.target,
            )?
        } else {
            self.content_sessions
                .session(&member)
                .and_then(|s| s.clip())
                .ok_or("This displayed document cannot supply a clip.")?
        };
        Ok(ClipOffer {
            id: uuid::Uuid::new_v4(),
            pane,
            dom,
            context,
            context_admission: self.app.pane_context.admission(),
            generation: self.app.source_capture_generation,
            admission,
            note,
            clip,
            target,
        })
    }

    /// One process-only offer replaces the previous one whenever any owner or
    /// source witness changes. No tokens, buffers or grants become pane data.
    pub(super) fn research_clip_offer(
        &mut self,
        pane: PaneId,
        dom: uuid::Uuid,
    ) -> (Option<String>, Option<uuid::Uuid>, String) {
        let proposed = self.proposed_clip_offer(pane, dom);
        match proposed {
            Ok(mut next) => {
                if let Some(previous) = self.clip_offers.get(&pane)
                    && previous.same_source(&next)
                {
                    if previous.target.same_admission(&next.target) {
                        return (
                            Some(previous.target.target().into()),
                            (!matches!(
                                previous.target.status(),
                                crate::knot_authoring::KnotClipStatus::Sending
                            ))
                            .then_some(previous.id),
                            previous.target.status().label(),
                        );
                    }
                    next.target.keep_status_from(&previous.target);
                }
                let result = (
                    Some(next.target.target().into()),
                    (!matches!(
                        next.target.status(),
                        crate::knot_authoring::KnotClipStatus::Sending
                    ))
                    .then_some(next.id),
                    next.target.status().label(),
                );
                self.clip_offers.insert(pane, next);
                result
            },
            Err(reason) => {
                self.clip_offers.remove(&pane);
                let target = self
                    .app
                    .research_note()
                    .ok()
                    .flatten()
                    .map(|n| n.address)
                    .or_else(|| self.knot_clip.as_ref().map(|h| h.target().to_string()));
                (target, None, reason)
            },
        }
    }

    pub(super) fn clip_document_to_knot(&mut self, id: uuid::Uuid) {
        let Some(offered) = self.clip_offers.values().find(|o| o.id == id).cloned() else {
            self.research_note_refusal(
                "This clip control is stale; use the current Inspector.".into(),
            );
            return;
        };
        let visible = self
            .surface_plan()
            .into_iter()
            .chain(
                self.lens_windows
                    .values()
                    .flat_map(|lens| self.lens_plan(lens.ordinal, lens.width, lens.height)),
            )
            .any(|surface| {
                surface.kind == crate::surface::SurfaceKind::Pane(offered.pane)
                    && surface.rect.w > 0.0
                    && surface.rect.h > 0.0
            });
        if !visible
            || !matches!(
                self.pane_content(offered.pane),
                Some(crate::panes::PaneContent::Inspector)
            )
        {
            self.research_note_refusal("The offered Inspector is no longer visible.".into());
            return;
        }
        let current_dom = self
            .renderers
            .inspector
            .get(&offered.pane)
            .map(|p| p.admission());
        if current_dom != Some(offered.dom)
            || self.research_clip_offer(offered.pane, offered.dom).1 != Some(id)
        {
            self.research_note_refusal(
                "The displayed source or note changed; use the current clip control.".into(),
            );
            return;
        }
        if let Err(reason) = offered.target.insert(offered.clip) {
            self.research_note_refusal(reason);
        }
    }
}
