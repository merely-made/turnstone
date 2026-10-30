// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Convert one fetch-actor answer into the app vocabulary, reattaching the
/// requesting node from the pending table. Content subresources are consumed
/// directly by the shell's retained-session port; other unmatched answers are
/// logged and dropped rather than guessed at.
pub fn update_from_fetch(update: FetchUpdate, pending: &mut PendingFetches) -> Option<Update> {
    match update {
        FetchUpdate::PageProgress(progress) => {
            let Some(pending_page) = pending.page(progress.request) else {
                tracing::warn!(
                    request = progress.request,
                    "page progress without a pending requester; dropped"
                );
                return None;
            };
            (pending_page.kind == PendingPageKind::Page).then(|| Update::PageStreamed {
                request: progress.request,
                node: pending_page.node,
                url: pending_page.owner_url.clone(),
                response_url: progress.response_url,
                content_type: progress.content_type,
                bytes: progress.bytes,
            })
        },
        FetchUpdate::Page(outcome) => {
            let Some(pending_page) = pending.take_page(outcome.request) else {
                // The actor URL may carry a status-11 answer in its query.
                // A correlation miss is diagnosable without copying that
                // transient request address into retained logs.
                tracing::warn!("page completion without a pending requester; dropped");
                return None;
            };
            match outcome.result {
                Ok(fetched) => {
                    let fetched = FetchedPage {
                        content_type: fetched.content_type,
                        content_disposition: fetched.content_disposition,
                        bytes: fetched.bytes,
                        body: fetched.body,
                    };
                    Some(match pending_page.kind {
                        PendingPageKind::Page => Update::PageFetched {
                            request: outcome.request,
                            node: pending_page.node,
                            url: pending_page.owner_url,
                            result: Ok(fetched),
                        },
                        PendingPageKind::Feed => Update::FeedFetched {
                            node: pending_page.node,
                            url: pending_page.owner_url,
                            result: Ok(fetched),
                        },
                    })
                },
                Err(fetch::FetchFailure::InputRequired {
                    url: input_url,
                    prompt,
                    sensitive,
                }) => Some(match pending_page.kind {
                    PendingPageKind::Page => Update::SmolwebInputRequested {
                        request: outcome.request,
                        node: pending_page.node,
                        url: pending_page.owner_url,
                        input_url,
                        prompt,
                        sensitive,
                    },
                    PendingPageKind::Feed => Update::FeedFetched {
                        node: pending_page.node,
                        url: pending_page.owner_url,
                        result: Err(format!(
                            "feed refresh requires {} input: {prompt}",
                            if sensitive {
                                "sensitive"
                            } else {
                                "interactive"
                            }
                        )),
                    },
                }),
                Err(fetch::FetchFailure::ClientCertificateRequired {
                    url: identity_url,
                    prompt,
                    code,
                }) => {
                    if pending_page.kind == PendingPageKind::Feed {
                        let status = code.map(|code| format!(" ({code})")).unwrap_or_default();
                        Some(Update::FeedFetched {
                            node: pending_page.node,
                            url: pending_page.owner_url,
                            result: Err(format!(
                                "feed refresh requires a client certificate{status}: {prompt}"
                            )),
                        })
                    } else if pending_page.identity_used {
                        let status = code.map(|code| format!(" ({code})")).unwrap_or_default();
                        Some(Update::PageFetched {
                            request: outcome.request,
                            node: pending_page.node,
                            url: pending_page.owner_url,
                            result: Err(format!("client certificate rejected{status}: {prompt}")),
                        })
                    } else {
                        Some(Update::GeminiIdentityRequested {
                            request: outcome.request,
                            node: pending_page.node,
                            url: pending_page.owner_url,
                            identity_url,
                            prompt,
                        })
                    }
                },
                Err(fetch::FetchFailure::CertificateChanged {
                    url: fetch_url,
                    target,
                    pinned,
                    seen,
                }) => Some(match pending_page.kind {
                    PendingPageKind::Page => Update::GeminiCertificateChanged {
                        request: outcome.request,
                        node: pending_page.node,
                        url: pending_page.owner_url,
                        fetch_url,
                        target,
                        pinned,
                        seen,
                    },
                    PendingPageKind::Feed => Update::FeedFetched {
                        node: pending_page.node,
                        url: pending_page.owner_url,
                        result: Err(format!(
                            "Gemini certificate changed for {target}; open the source to review it"
                        )),
                    },
                }),
                Err(fetch::FetchFailure::Failed(error)) => Some(match pending_page.kind {
                    PendingPageKind::Page => Update::PageFetched {
                        request: outcome.request,
                        node: pending_page.node,
                        url: pending_page.owner_url,
                        result: Err(error),
                    },
                    PendingPageKind::Feed => Update::FeedFetched {
                        node: pending_page.node,
                        url: pending_page.owner_url,
                        result: Err(error),
                    },
                }),
                Err(fetch::FetchFailure::Cancelled) => Some(match pending_page.kind {
                    PendingPageKind::Page => Update::PageStopped {
                        request: outcome.request,
                        node: pending_page.node,
                        url: pending_page.owner_url,
                    },
                    PendingPageKind::Feed => Update::FeedFetched {
                        node: pending_page.node,
                        url: pending_page.owner_url,
                        result: Err("cancelled".to_string()),
                    },
                }),
            }
        },
        FetchUpdate::Favicon(outcome) => {
            let Some(pending_favicon) = pending.take_favicon(outcome.request) else {
                tracing::warn!(
                    request = outcome.request,
                    "favicon completion without a pending requester; dropped"
                );
                return None;
            };
            match outcome.result {
                Err(error) => {
                    tracing::debug!(request = outcome.request, url = %pending_favicon.owner_url, %error, "best-effort favicon fetch failed");
                    None
                },
                Ok(bytes) => Some(Update::FaviconFetched {
                    node: pending_favicon.node,
                    owner_url: pending_favicon.owner_url,
                    bytes,
                }),
            }
        },
        FetchUpdate::Subresource(_) => None,
        FetchUpdate::Submission(outcome) => {
            let Some(source) = pending.submissions.remove(&outcome.request) else {
                tracing::warn!(
                    request = outcome.request,
                    "submission completion without a pending requester; dropped"
                );
                return None;
            };
            let result = outcome
                .result
                .map(|answer| match answer {
                    fetch::SubmissionAnswer::Success(fetched) => {
                        crate::action::SmolwebSubmissionReceipt::Success(FetchedPage {
                            content_type: fetched.content_type,
                            content_disposition: fetched.content_disposition,
                            bytes: fetched.bytes,
                            body: fetched.body,
                        })
                    },
                    fetch::SubmissionAnswer::Redirect(target) => {
                        crate::action::SmolwebSubmissionReceipt::Redirect(target)
                    },
                })
                .map_err(|error| error.to_string());
            Some(Update::SmolwebSubmitted {
                request: outcome.request,
                source,
                target: outcome.url,
                result,
            })
        },
    }
}
