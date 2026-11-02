// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    /// The find gate's refusing shape. An engine without page zoom never
    /// offers the rows, so this only catches a chord or a script aimed at one.
    pub(super) fn refuse_unzoomable(&mut self, member: Uuid) -> bool {
        if !matches!(
            self.content.get(member),
            Some(crate::content::NodeContent::Live)
        ) {
            self.note_page_zoom_refusal("the node has no live document");
            return false;
        }
        let Some(status) = self
            .content
            .facts(member)
            .map(|facts| facts.capabilities.page_zoom.clone())
        else {
            self.note_page_zoom_refusal("live engine did not report document capabilities");
            return false;
        };
        if !status.is_available() {
            self.events.push(AppEvent::AffordanceUnavailable {
                what: "page-zoom",
                target: status.describe(),
            });
            return false;
        }
        true
    }

    pub(super) fn note_page_zoom_refusal(&mut self, reason: &str) {
        self.events.push(AppEvent::AffordanceUnavailable {
            what: "page-zoom",
            target: reason.to_string(),
        });
    }

    pub(in super::super) fn toggle_node_content(&mut self) -> Vec<Effect> {
        // The flip targets the focused node; no focus, no-op (the
        // caption chip tells the user what would flip).
        // Resolve the node by MEMBER, not by URL round-trip: two
        // nodes may share a URL (the sample graph + an open), and
        // get_node_by_url picks arbitrarily between them.
        let Some(target) = self.browser_command_target() else {
            return Vec::new();
        };
        let (node, url) = target;
        if self.content.flip_spawns(node) {
            self.content.note_requested(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "requested".to_string(),
            });
            vec![Effect::SpawnContent { node, url }, Effect::Redraw]
        } else {
            self.content.note_closed(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "closed".to_string(),
            });
            let mut effects = self.invalidate_document_find_for(node);
            effects.push(Effect::CloseContent { node });
            effects.push(Effect::Redraw);
            effects
        }
    }

    pub(in super::super) fn reload_focused(&mut self) -> Vec<Effect> {
        let Some(target) = self.browser_command_target() else {
            return vec![Effect::Redraw];
        };
        let (node, url) = target;
        self.events.push(AppEvent::Reloaded(url.clone()));
        self.saved_readers.remove(&node);
        self.link_preview = None;
        let mut find_effects = self.invalidate_document_find_for(node);
        find_effects.extend(self.invalidate_micron_submission());

        if self.node_uses_web_surface(node) {
            self.content.take_surface_traversal(node);
            self.content.note_surface_started(node);
            find_effects.push(Effect::ControlContent {
                node,
                control: crate::action::ContentControl::Reload,
            });
            find_effects.push(Effect::Redraw);
            return find_effects;
        }

        self.content.forget_fetched(node);
        let mut effects = find_effects;
        if crate::browse::is_fetchable(&url) {
            effects.push(self.fetch_page_effect(node, url.clone(), url.clone()));
        }
        // A live, in-flight, or failed session restarts fresh. A failure has
        // no shell handle, but it is still a requested content surface: leave
        // it in Requested so the successful replacement body spawns instead
        // of merely replacing the cache behind its error tile.
        if matches!(
            self.content.get(node),
            Some(
                crate::content::NodeContent::Live
                    | crate::content::NodeContent::Requested
                    | crate::content::NodeContent::Failed(_)
            )
        ) {
            self.content.note_requested(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "requested".to_string(),
            });
            effects.push(Effect::CloseContent { node });
            effects.push(Effect::SpawnContent { node, url });
        }
        effects.push(Effect::Redraw);
        effects
    }

    pub(in super::super) fn stop_focused(&mut self) -> Vec<Effect> {
        let Some((node, _)) = self.browser_command_target() else {
            return vec![Effect::Redraw];
        };
        self.link_preview = None;
        if let Some(request) = self.content.active_fetch(node) {
            if self.content.stop_fetch(node, request) {
                self.events.push(AppEvent::ContentState {
                    node,
                    state: "stopped".to_string(),
                });
                return vec![Effect::CancelPage { request, node }, Effect::Redraw];
            }
        }
        if self.node_uses_web_surface(node) && self.content.fetch_in_progress(node) {
            self.content.note_surface_stopped(node);
            self.events.push(AppEvent::ContentState {
                node,
                state: "stopped".to_string(),
            });
            return vec![
                Effect::ControlContent {
                    node,
                    control: crate::action::ContentControl::Stop,
                },
                Effect::Redraw,
            ];
        }
        vec![Effect::Redraw]
    }

    /// One committed place prompt: the typed line becomes the paired action.
    pub(super) fn commit_place_prompt(&mut self, prompt: crate::ui::PlacePrompt) -> Vec<Effect> {
        use crate::ui::PlacePrompt;
        let text = self.omnibar.text.trim().to_string();
        let action = match prompt {
            PlacePrompt::FoundPlace => Action::FoundPlace { name: text.clone() },
            PlacePrompt::ExportCard => Action::ExportPlaceCard { path: text.clone() },
            PlacePrompt::RefreshContact => Action::RefreshPlaceContact { path: text.clone() },
            PlacePrompt::OfferPrekey => Action::OfferPlacePrekey { path: text.clone() },
            PlacePrompt::Invite | PlacePrompt::InviteReader => {
                match crate::place::split_invite_lifetime(&text) {
                    Ok((path, lifetime_ms)) => Action::InviteToPlace {
                        path: path.to_string(),
                        access: if prompt == PlacePrompt::Invite {
                            crate::place::PlaceInviteAccess::Writer
                        } else {
                            crate::place::PlaceInviteAccess::Reader
                        },
                        lifetime_ms,
                    },
                    Err(reason) => {
                        self.omnibar = OmnibarState::default();
                        return self.refuse_place(reason);
                    },
                }
            },
            PlacePrompt::Join => Action::JoinPlaceFile { path: text.clone() },
            PlacePrompt::SendMessage => match self.place.binding() {
                Some(binding) => Action::SendPlaceMessage {
                    channel: binding.default_channel.clone(),
                    body: text.clone(),
                },
                None => {
                    self.omnibar = OmnibarState::default();
                    let mut fx = self.refuse_place("this session is not in a place");
                    fx.push(Effect::Redraw);
                    return fx;
                },
            },
        };
        let label = match prompt {
            PlacePrompt::FoundPlace => "Found place",
            PlacePrompt::ExportCard => "Export place card",
            PlacePrompt::RefreshContact => "Refresh place contact",
            PlacePrompt::OfferPrekey => "Offer place pre-key",
            PlacePrompt::Invite => "Invite to place",
            PlacePrompt::InviteReader => "Invite to place as reader",
            PlacePrompt::Join => "Join place",
            PlacePrompt::SendMessage => "Send place message",
        };
        let target = self.fallback_shell_context();
        let entry = self.shell.record_omnibar(
            ShellInput::Omnibar(text),
            ShellIntent::Command {
                label: label.into(),
                action: action.clone(),
            },
            target,
            EntryPrivacy::Ordinary,
        );
        self.omnibar = OmnibarState::default();
        if self.focus == FocusTarget::Chrome {
            self.focus = FocusTarget::Graph(self.default_graph_pane());
        }
        let mut fx = self.update(action);
        self.shell.complete(
            entry,
            ShellOutcome::Completed {
                summary: label.to_lowercase(),
            },
        );
        fx.push(Effect::Redraw);
        fx
    }

    /// Address-field navigation follows the current main view. Other graph
    /// commands keep their independent selection and never retarget a reader.
    pub(super) fn open_omnibar_address(&mut self, url: String, reading: bool) -> Vec<Effect> {
        let start = self.events.len();
        let mut effects = self.update(Action::OpenAddress(url.clone()));
        let admitted = self.events[start..]
            .iter()
            .any(|event| matches!(event, AppEvent::AddressOpened(address) if address == &url));
        if reading && admitted {
            let pane = self
                .focused_graph_pane()
                .unwrap_or_else(|| self.default_graph_pane());
            if let Some(member) = self.graph_pane_focused_member(pane) {
                effects.extend(self.read_graph_member(member));
            }
        }
        effects
    }
}
