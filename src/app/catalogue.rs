// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::action::{Action, Effect};
use crate::place::catalogue::CatalogueAction as SiteAction;
use crate::place::catalogue::{CatalogueSiteDraft, subject_name};
use crate::ui::{OmnibarMode, OmnibarState, PlacePrompt, Suggestion};
impl App {
    pub(super) fn open_catalogue_site(&mut self, digest: [u8; 32]) -> Vec<Effect> {
        let effects =
            self.run_place_command(crate::place::worker::PlaceCommand::OpenCatalogue { digest });
        if effects
            .iter()
            .any(|e| matches!(e, Effect::RunPlaceCommand { .. }))
        {
            self.catalogue_open = Some((self.next_place_request, self.command_observation()));
        }
        effects
    }
    pub(super) fn finish_catalogue_navigation(
        &mut self,
        request: u64,
        address: String,
    ) -> Option<String> {
        let (pending, observation) = self.catalogue_open.as_ref()?;
        if *pending != request {
            return None;
        }
        let valid =
            request == self.next_place_request && *observation == self.command_observation();
        self.catalogue_open = None;
        valid.then_some(address)
    }
    pub(super) fn begin_advertise_site(&mut self, subject: u8) -> Vec<Effect> {
        if subject > 9 {
            return self.refuse_place("invalid Scroll subject class");
        }
        let Some(address) = self.focused_address() else {
            return self.refuse_place("focus a public site first");
        };
        if self.place.binding().is_none() {
            return self.refuse_place("join a place first");
        }
        self.begin_place_prompt(PlacePrompt::AdvertiseSite {
            subject,
            expected_address: *blake3::hash(address.as_bytes()).as_bytes(),
        })
    }
    pub(super) fn commit_site_advertisement(
        &mut self,
        expected: [u8; 32],
        subject: u8,
        abstract_text: String,
    ) -> Vec<Effect> {
        let Some(address) = self.focused_address() else {
            return self.refuse_place("the site focus changed; start the advertisement again");
        };
        if *blake3::hash(address.as_bytes()).as_bytes() != expected {
            return self.refuse_place("the site address changed; start the advertisement again");
        }
        let title = if abstract_text.trim().is_empty() {
            address.chars().take(120).collect()
        } else {
            abstract_text.chars().take(120).collect()
        };
        self.run_place_command(crate::place::worker::PlaceCommand::AdvertiseSite(
            CatalogueSiteDraft {
                address,
                title,
                abstract_text,
                scroll_subject: Some(subject),
            },
        ))
    }
    pub(super) fn browse_sites(&mut self, subject: Option<u8>) -> Vec<Effect> {
        if subject.is_some_and(|v| v > 9) {
            return self.refuse_place("invalid Scroll subject class");
        }
        self.omnibar = OmnibarState {
            open: true,
            mode: OmnibarMode::CatalogueSites { subject },
            ..Default::default()
        };
        self.reflow_omnibar();
        vec![Effect::Redraw]
    }
    pub(super) fn catalogue_actions(&self) -> Vec<(String, Action)> {
        let mut rows = vec![(
            "Browse discovered sites".into(),
            Action::Catalogue(SiteAction::BrowseSites { subject: None }),
        )];
        for class in 0..=9 {
            rows.push((
                format!("Browse sites: {}", subject_name(class)),
                Action::Catalogue(SiteAction::BrowseSites {
                    subject: Some(class),
                }),
            ));
            if let crate::place::PlaceState::Offline { snapshot, .. } = &self.place
                && snapshot.members.iter().any(|m| {
                    m.root == snapshot.personae_root
                        && m.access == crate::place::PlaceMemberAccess::Manage
                })
            {
                rows.push((
                    format!("Advertise focused public site: {}", subject_name(class)),
                    Action::Catalogue(SiteAction::BeginAdvertiseSite { subject: class }),
                ));
            }
        }
        if let crate::place::PlaceState::Offline { snapshot, .. } = &self.place {
            for site in &snapshot.catalogues {
                if site.provider == snapshot.personae_root {
                    rows.push((
                        format!("Withdraw site advertisement: {}", site.title),
                        Action::Catalogue(SiteAction::WithdrawSite {
                            collection: site.collection,
                        }),
                    ));
                }
            }
        }
        rows
    }
    pub(super) fn recompute_catalogue_suggestions(&mut self, subject: Option<u8>, limit: usize) {
        let query = self.omnibar.text.trim().to_lowercase();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(u64::MAX);
        let label = subject.map(subject_name).unwrap_or("All subjects");
        let mut rows = vec![Suggestion::Prompt(format!(
            "Discovered sites: {label}; opening rechecks current access"
        ))];
        if let crate::place::PlaceState::Offline { snapshot, .. } = &self.place {
            if let Some(error) = &snapshot.catalogue_error {
                rows.push(Suggestion::Prompt(format!(
                    "Site catalogue unavailable: {error}"
                )));
            }
            for site in &snapshot.catalogues {
                if site.expires_at_ms <= now
                    || subject.is_some_and(|v| site.scroll_subject != Some(v))
                {
                    continue;
                }
                let text = format!("{} {} {}", site.title, site.abstract_text, site.address)
                    .to_lowercase();
                if !query.split_whitespace().all(|part| text.contains(part)) {
                    continue;
                }
                rows.push(Suggestion::Act {
                    label: format!("{} · {}", site.title, site.address),
                    action: Action::Catalogue(SiteAction::OpenDiscoveredSite {
                        digest: site.digest,
                    }),
                });
            }
        }
        if rows.len() == 1 {
            rows.push(Suggestion::Hint("no permitted site advertisements match"));
        }
        if limit == 1 && rows.iter().any(|r| matches!(r, Suggestion::Act { .. })) {
            rows.retain(|r| matches!(r, Suggestion::Act { .. }));
        }
        rows.truncate(limit.max(1));
        self.omnibar.suggestions = rows;
        self.omnibar.selected = self
            .omnibar
            .suggestions
            .iter()
            .position(|s| matches!(s, Suggestion::Act { .. }))
            .unwrap_or(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> App {
        let mut app = App::test_stub();
        let binding = crate::place::PlaceBindingV1::new(
            crate::place::PlaceId([1; 32]),
            crate::place::SharedContainerId([2; 32]),
            crate::place::ChatSpaceId([3; 32]),
            "hall",
        )
        .unwrap();
        let mut snapshot = crate::place::OfflinePlaceSnapshot::default();
        snapshot.catalogues = vec![crate::place::catalogue::DiscoveredSite {
            digest: [4; 32],
            provider: [5; 32],
            collection: [6; 32],
            sequence: 1,
            address: "scroll://garden.test/".into(),
            title: "Garden".into(),
            abstract_text: "Painting and music".into(),
            scroll_subject: Some(7),
            expires_at_ms: u64::MAX,
        }];
        app.place = crate::place::PlaceState::Offline {
            binding,
            generation: 7,
            snapshot,
        };
        app
    }
    #[test]
    fn catalogue_ui_filters_subject_and_abstract_then_requests_owner_validation() {
        let mut app = fixture();
        app.update(Action::Catalogue(SiteAction::BrowseSites {
            subject: Some(7),
        }));
        app.update(Action::OmnibarInsert("painting".into()));
        assert!(app.omnibar.suggestions.iter().any(|s|matches!(s,Suggestion::Act {action:Action::Catalogue(SiteAction::OpenDiscoveredSite {digest}),..} if *digest==[4;32])));
        let effects = app.update(Action::Catalogue(SiteAction::OpenDiscoveredSite {
            digest: [4; 32],
        }));
        assert!(effects.iter().any(|e|matches!(e,Effect::RunPlaceCommand {generation:7,command:crate::place::worker::PlaceCommand::OpenCatalogue {digest},..} if *digest==[4;32])));
        app.update(Action::Catalogue(SiteAction::BrowseSites {
            subject: Some(5),
        }));
        assert!(
            !app.omnibar
                .suggestions
                .iter()
                .any(|s| matches!(s, Suggestion::Act { .. }))
        );
    }
    #[test]
    fn expired_site_and_changed_focus_do_not_open_or_advertise() {
        let mut app = fixture();
        if let crate::place::PlaceState::Offline { snapshot, .. } = &mut app.place {
            snapshot.catalogues[0].expires_at_ms = 0;
        }
        app.update(Action::Catalogue(SiteAction::BrowseSites { subject: None }));
        assert!(
            !app.omnibar
                .suggestions
                .iter()
                .any(|s| matches!(s, Suggestion::Act { .. }))
        );
        let effects = app.update(Action::Catalogue(SiteAction::CommitSiteAdvertisement {
            expected_address: [99; 32],
            subject: 7,
            abstract_text: "old focus".into(),
        }));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::RunPlaceCommand { .. }))
        );
    }
    #[test]
    fn catalogue_navigation_requires_matching_request_session_generation_and_focus() {
        use crate::action::Update;
        let mut app = fixture();
        app.update(Action::OpenAddress("https://starting.test/".into()));
        app.update(Action::Catalogue(SiteAction::OpenDiscoveredSite {
            digest: [4; 32],
        }));
        let request = app.next_place_request;
        let completion = |session, generation, request| Update::PlaceCommandDone {
            session,
            generation,
            request,
            result: Ok(crate::place::OfflinePlaceSnapshot {
                catalogue_navigation: Some("https://garden.test/".into()),
                ..Default::default()
            }),
        };
        app.apply_update(completion(crate::panes::SessionId::new(), 7, request));
        app.apply_update(completion(app.session_id, 8, request));
        app.apply_update(completion(app.session_id, 7, request.wrapping_sub(1)));
        assert_eq!(
            app.focused_address().as_deref(),
            Some("https://starting.test/")
        );
        app.apply_update(completion(app.session_id, 7, request));
        assert_eq!(
            app.focused_address().as_deref(),
            Some("https://garden.test/")
        );
        let consumed = app.next_place_request;
        app.apply_update(completion(app.session_id, 7, request));
        assert_eq!(app.next_place_request, consumed);
        app.update(Action::Catalogue(SiteAction::OpenDiscoveredSite {
            digest: [4; 32],
        }));
        let request = app.next_place_request;
        app.update(Action::OpenAddress("https://elsewhere.test/".into()));
        app.apply_update(completion(app.session_id, 7, request));
        assert_eq!(
            app.focused_address().as_deref(),
            Some("https://elsewhere.test/")
        );
    }
    #[test]
    fn catalogue_publication_requires_host_ring() {
        use crate::ring::{Ring, ring_of};
        assert_eq!(
            ring_of(&Action::Catalogue(SiteAction::BeginAdvertiseSite {
                subject: 7
            })),
            Ring::HostOnly
        );
        assert_eq!(
            ring_of(&Action::Catalogue(SiteAction::CommitSiteAdvertisement {
                expected_address: [0; 32],
                subject: 7,
                abstract_text: "public".into()
            })),
            Ring::HostOnly
        );
    }
}
