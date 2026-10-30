// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// Fold one typed service answer into state.
    pub fn apply_update(&mut self, update: Update) -> Vec<Effect> {
        if self.session_load_refused() {
            return Vec::new();
        }
        let capture_generation = match &update {
            Update::SourceDocumentCaptured {
                capture_generation, ..
            }
            | Update::SourceCaptureSaved {
                capture_generation, ..
            }
            | Update::SourceCaptureRecovered {
                capture_generation, ..
            }
            | Update::SourceCaptureRevalidated {
                capture_generation, ..
            }
            | Update::SourceCaptureRecoveryFailed {
                capture_generation, ..
            } => Some(*capture_generation),
            _ => None,
        };
        if capture_generation.is_some_and(|generation| generation != self.source_capture_generation)
        {
            return Vec::new();
        }
        match update {
            Update::SavedSourcesLoaded { .. }
            | Update::SavedSourceRead { .. }
            | Update::FeedFetched { .. }
            | Update::PageStreamed { .. }
            | Update::PageFetched { .. }
            | Update::SmolwebSubmitted { .. }
            | Update::SmolwebInputRequested { .. }
            | Update::GeminiIdentityRequested { .. } => self.update_saved_sources_loaded(update),
            Update::GeminiCertificateChanged { .. }
            | Update::PageStopped { .. }
            | Update::FaviconFetched { .. }
            | Update::DownloadStored { .. }
            | Update::ContentSpawned { .. }
            | Update::ReadingFindChanged { .. }
            | Update::DocumentFindChanged { .. }
            | Update::PermissionRequested { .. }
            | Update::AuthenticationRequested { .. }
            | Update::PermissionRequestFinished { .. }
            | Update::AuthenticationRequestFinished { .. }
            | Update::UserAgentRequestWithdrawn { .. }
            | Update::ContentSourceCaptured { .. }
            | Update::ContentFailed { .. }
            | Update::BinListed { .. }
            | Update::BinFailed { .. }
            | Update::RecallHits { .. }
            | Update::RecallFailed { .. }
            | Update::SourceDocumentCaptured { .. }
            | Update::SourceCaptureRevalidated { .. }
            | Update::SourceCaptureRecoveryFailed { .. }
            | Update::SourceCaptureRecovered { .. }
            | Update::SourceCaptureSaved { .. }
            | Update::PlaceDocumentVisit { .. }
            | Update::PlaceLanesAdvanced { .. }
            | Update::PlaceCommandDone { .. }
            | Update::PlaceCollectionSet { .. }
            | Update::PlaceFounded { .. }
            | Update::PlacePrekeyOffered { .. } => self.update_gemini_certificate_changed(update),
            Update::PlaceInvited { .. }
            | Update::PlaceJoined { .. }
            | Update::PlaceOpened { .. } => self.update_place_invited(update),
        }
    }
}
