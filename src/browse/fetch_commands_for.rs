// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Translate an effect into the fetch actor's command, if it is fetch-shaped.
/// The shell notes the requester in [`PendingFetches`] and commands the
/// actor; the mapping stays beside the enrichment it feeds.
pub fn fetch_commands_for(effect: &Effect, pending: &mut PendingFetches) -> Vec<FetchCommand> {
    match effect {
        Effect::FetchPage {
            request,
            supersedes,
            node,
            url,
            owner_url,
            identity,
        } => {
            let mut commands = Vec::with_capacity(2);
            if let Some(old) = supersedes {
                pending.cancel_page(*old);
                commands.push(FetchCommand::CancelPage { request: *old });
            }
            pending.note_page(*request, url, *node, owner_url, identity.is_some());
            commands.push(FetchCommand::Page {
                request: *request,
                url: url.clone(),
                identity: identity.clone(),
            });
            commands
        },
        Effect::CancelPage { request, .. } => {
            pending.cancel_page(*request);
            vec![FetchCommand::CancelPage { request: *request }]
        },
        Effect::FetchFeed {
            node,
            url,
            identity,
        } => {
            let request = fetch::next_fetch_request_id();
            pending.note_feed(request, url, *node, identity.is_some());
            vec![FetchCommand::Page {
                request,
                url: url.clone(),
                identity: identity.clone(),
            }]
        },
        Effect::FetchFavicon {
            node,
            owner_url,
            url,
        } => {
            let request = fetch::next_fetch_request_id();
            pending.note_favicon(request, owner_url, *node);
            vec![FetchCommand::Favicon {
                request,
                url: url.clone(),
            }]
        },
        Effect::SubmitSmolweb {
            request,
            source,
            target,
            protocol,
            body,
            mime,
            token,
            identity,
        } => {
            pending.submissions.insert(*request, *source);
            let submission = match protocol {
                crate::ui::SmolwebSubmissionProtocol::Titan => fetch::SmolwebSubmission::Titan {
                    url: target.clone(),
                    body: body.clone(),
                    mime: mime.clone(),
                    token: token.as_ref().map(|token| token.as_str().to_string()),
                    identity: identity.clone(),
                },
                crate::ui::SmolwebSubmissionProtocol::Spartan => {
                    fetch::SmolwebSubmission::Spartan {
                        url: target.clone(),
                        body: body.clone(),
                    }
                },
            };
            vec![FetchCommand::Submit {
                request: *request,
                submission,
            }]
        },
        _ => Vec::new(),
    }
}
