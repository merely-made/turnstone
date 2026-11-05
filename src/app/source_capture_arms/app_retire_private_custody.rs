// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Drop private projections and invalidate every queued source reply before
    /// the Shell drains its actors. Durable records remain for explicit reopen.
    pub(crate) fn retire_private_custody(&mut self) -> Vec<Effect> {
        self.clear_private_research();
        self.source_capture_generation = uuid::Uuid::new_v4();
        self.identity = None;
        self.catalogue_open = None;
        // Derived catalogue labels and unfinished disclosure prompts belong to
        // the retired identity, even when they were already in the palette.
        let catalogue_ui = matches!(
            self.omnibar.mode,
            crate::ui::OmnibarMode::CatalogueSites { .. }
                | crate::ui::OmnibarMode::Place(crate::ui::PlacePrompt::AdvertiseSite { .. })
        ) || self.omnibar.suggestions.iter().any(|row| {
            matches!(
                row,
                crate::ui::Suggestion::Act {
                    action: crate::action::Action::Catalogue(_),
                    ..
                }
            )
        });
        if catalogue_ui {
            self.omnibar = Default::default();
        }

        self.research.set_available(false);
        self.saved_sources = Default::default();
        self.research_search = Default::default();
        self.source_captures.clear();
        self.content.clear_source_documents();
        if let crate::place::PlaceState::Offline { snapshot, .. } = &mut self.place {
            snapshot.captured = Default::default();
            snapshot.catalogues.retain(|site| {
                site.visibility == crate::place::catalogue::CatalogueVisibility::Public
            });
            snapshot.catalogue_navigation = None;
        }
        self.events.retain(|event| {
            !matches!(
                event,
                AppEvent::SourceDocumentCaptureProgress { .. }
                    | AppEvent::SourceDocumentCaptured { .. }
                    | AppEvent::SourceDocumentCaptureFailed { .. }
                    | AppEvent::SourceDocumentCaptureUnattached { .. }
            )
        });
        let reader_nodes: Vec<_> = self
            .saved_readers
            .keys()
            .chain(self.research_clip_readers.keys())
            .copied()
            .collect();
        self.saved_readers.clear();
        self.research_clip_readers.clear();
        let mut effects = self.cancel_smolweb_conversation();
        effects.push(Effect::CancelResearchSearch);
        for node in reader_nodes {
            self.content.note_failed(
                node,
                "Unlock the profile to read this private source".into(),
            );
            effects.push(Effect::CloseContent { node });
        }
        if matches!(
            self.omnibar.mode,
            crate::ui::OmnibarMode::SavedSources | crate::ui::OmnibarMode::CapturedPages
        ) {
            self.omnibar.suggestions.clear();
            self.omnibar.selected = 0;
        }
        effects.push(Effect::Redraw);
        effects
    }
}
