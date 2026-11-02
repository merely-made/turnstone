// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl AppEvent {
    /// A grep-friendly one-line rendering (what `assert event` matches).
    pub fn describe(&self) -> String {
        match self {
            AppEvent::PhysicsRefused(refusal) => format!(
                "physics-refused {} {}: {}",
                refusal.law.id(),
                refusal
                    .refused
                    .iter()
                    .map(|overlay| overlay.id())
                    .collect::<Vec<_>>()
                    .join(","),
                refusal.reason,
            ),
            AppEvent::AddressOpened(url) => format!("address-opened {url}"),
            AppEvent::ContentNavigated { node, url } => {
                format!("content-navigated {node} {url}")
            },
            AppEvent::ContentTitleChanged { node, title } => {
                format!("content-title {node} {title}")
            },
            AppEvent::AuxiliaryNavigableRequested { node, url } => {
                format!("auxiliary-navigable {node} {url}")
            },
            AppEvent::SurfaceCursorChanged { node, shape } => {
                format!("surface-cursor {node} {shape}")
            },
            AppEvent::PageDragRequested { node, types } => {
                format!("page-drag-requested {node} types={types}")
            },
            AppEvent::PermissionRequested {
                node,
                id,
                origin,
                descriptors,
            } => format!(
                "permission-requested {node} request={} {origin} {}",
                id.get(),
                descriptors
                    .iter()
                    .map(|descriptor| format!("{descriptor:?}"))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            AppEvent::AuthenticationRequested {
                node,
                id,
                host,
                realm,
                scheme,
            } => format!(
                "authentication-requested {node} request={} {host} {scheme} {}",
                id.get(),
                realm.as_deref().unwrap_or("")
            ),
            AppEvent::PermissionAnswered {
                node,
                id,
                answer,
                remembered,
                succeeded,
            } => format!(
                "permission-answered {node} request={} answer={answer:?} remembered={remembered} succeeded={succeeded}",
                id.get()
            ),
            AppEvent::AuthenticationAnswered {
                node,
                id,
                supplied_credentials,
                remembered_for_process,
                succeeded,
            } => format!(
                "authentication-answered {node} request={} supplied={supplied_credentials} process-memory={remembered_for_process} succeeded={succeeded}",
                id.get()
            ),
            AppEvent::UserAgentRequestWithdrawn {
                node,
                id,
                kind,
                reason,
            } => format!(
                "user-agent-request-withdrawn {node} request={} kind={kind} reason={reason}",
                id.get()
            ),
            AppEvent::SmolwebInputRequested {
                node,
                prompt,
                sensitive,
            } => format!("smolweb-input-requested {node} sensitive={sensitive} {prompt}"),
            AppEvent::SmolwebInputSubmitted { node, sensitive } => {
                format!("smolweb-input-submitted {node} sensitive={sensitive}")
            },
            AppEvent::SmolwebSubmissionComposed { target } => {
                format!("smolweb-submission-composed {target}")
            },
            AppEvent::SmolwebSubmissionStarted { target, bytes } => {
                format!("smolweb-submission-started {target} bytes={bytes}")
            },
            AppEvent::SmolwebSubmissionSucceeded { target, outcome } => {
                format!("smolweb-submission-succeeded {target} {outcome}")
            },
            AppEvent::SmolwebSubmissionFailed { target, error } => {
                format!("smolweb-submission-failed {target} {error}")
            },
            AppEvent::DownloadStarted { node, url, bytes } => {
                format!("download-started {url} bytes={bytes} node={node}")
            },
            AppEvent::DownloadCompleted {
                node,
                destination,
                content_hash,
            } => format!(
                "download-completed {node} hash={} {destination}",
                content_hash.get(..12).unwrap_or(content_hash)
            ),
            AppEvent::DownloadFailed { node, error } => {
                format!("download-failed {node} {error}")
            },
            AppEvent::SourceDocumentCaptureProgress {
                request,
                node,
                session,
                stage,
                detail,
                stored,
            } => format!(
                "source-document-capture-progress request={request} session={session:?} node={node} stage={stage} stored={stored:?} {}",
                detail.as_deref().unwrap_or("")
            ),
            AppEvent::SourceDocumentCaptured {
                node,
                url,
                raw_manifest,
                annotation_manifest,
            } => format!(
                "source-document-captured {node} {url} raw={raw_manifest} annotation={annotation_manifest}"
            ),
            AppEvent::SourceDocumentCaptureFailed { node, url, error } => {
                format!("source-document-capture-failed {node} {url} {error}")
            },
            AppEvent::SourceDocumentCaptureUnattached {
                node,
                url,
                raw_manifest,
                annotation_manifest,
                reason,
            } => format!(
                "source-document-capture-unattached {node} {url} raw={raw_manifest} annotation={annotation_manifest} {reason}"
            ),
            AppEvent::GeminiIdentityRequested {
                node,
                origin,
                prompt,
            } => format!("gemini-identity-requested {node} {origin} {prompt}"),
            AppEvent::GeminiIdentityBound { node, origin } => {
                format!("gemini-identity-bound {node} {origin}")
            },
            AppEvent::GeminiCertificateChanged {
                node,
                target,
                pinned,
                seen,
            } => format!(
                "gemini-certificate-changed {node} {target} {} {}",
                pinned.get(..12).unwrap_or(pinned),
                seen.get(..12).unwrap_or(seen)
            ),
            AppEvent::NavigatedBack(url) => format!("nav-back {url}"),
            AppEvent::NavigatedForward(url) => format!("nav-forward {url}"),
            AppEvent::Reloaded(url) => format!("reloaded {url}"),
            AppEvent::FeedSubscribed { node, period } => {
                format!("feed-subscribed {node} {period}")
            },
            AppEvent::FeedUnsubscribed(node) => format!("feed-unsubscribed {node}"),
            AppEvent::FeedRefreshed {
                node,
                changed,
                unread,
            } => format!("feed-refreshed {node} changed={changed} unread={unread}"),
            AppEvent::FeedRefreshFailed { node, error } => {
                format!("feed-refresh-failed {node} {error}")
            },
            AppEvent::FeedEntryRead(node) => format!("feed-entry-read {node}"),
            AppEvent::RedshankRefused(message) => format!("redshank-refused {message}"),
            AppEvent::NodeKept(node) => format!("node-kept {node}"),
            AppEvent::NodeSpriteSet(node) => format!("sprite-set {node}"),
            AppEvent::ViewerChanged { node, viewer } => format!("viewer-changed {node} {viewer}"),
            AppEvent::PageZoomChanged { node, percent } => {
                format!("page-zoom-changed {node} {percent}%")
            },
            AppEvent::WindowOpened => "window-opened".to_string(),
            AppEvent::WindowClosed => "window-closed".to_string(),
            AppEvent::PaneTornOut(tag) => format!("pane-torn-out {tag}"),
            AppEvent::PaneFloated(tag) => format!("pane-floated {tag}"),
            AppEvent::PaneDocked(tag) => format!("pane-docked {tag}"),
            AppEvent::PaneReturned(tag) => format!("pane-returned {tag}"),
            AppEvent::TileTornOut(url) => format!("tile-torn-out {url}"),
            AppEvent::ProfileIdentityPending => "profile-identity-pending".to_string(),
            AppEvent::ProfileIdentityUnlocked(protection) => {
                format!("profile-identity-unlocked {protection}")
            },
            AppEvent::PlaceRefused(reason) => format!("place-refused {reason}"),
            AppEvent::PlaceRendezvousCopied(count) => {
                format!("place-rendezvous-copied {count}")
            },
            AppEvent::SessionSwitched(label) => format!("session-switched {label}"),
            AppEvent::SessionLoadRefused { error } => format!("session-load-refused {error}"),
            AppEvent::SessionClosed => "session-closed".to_string(),
            AppEvent::SessionForked => "session-forked".to_string(),
            AppEvent::SessionRecovered(label) => format!("session-recovered {label}"),
            AppEvent::DenizenStaged(label) => format!("denizen-staged {label}"),
            AppEvent::DenizenInstalled(label) => format!("denizen-installed {label}"),
            AppEvent::DenizenRan(label) => format!("denizen-ran {label}"),
            AppEvent::DenizenUninstalled(label) => format!("denizen-uninstalled {label}"),
            AppEvent::CascadeExhausted(names) => format!("cascade-exhausted {names}"),
            AppEvent::DenizenRefused(reason) => format!("denizen-refused {reason}"),
            AppEvent::SessionRenamed(label) => format!("session-renamed {label}"),
            AppEvent::NodeRemoved(url) => format!("node-removed {url}"),
            AppEvent::NodeRecovered(url) => format!("node-recovered {url}"),
            AppEvent::BinFailed(error) => format!("bin-failed {error}"),
            AppEvent::RecallFailed(error) => format!("recall-failed {error}"),
            AppEvent::RecycleBinEmptied(n) => format!("recycle-bin-emptied {n}"),
            AppEvent::PaneSectionToggled { section, added } => {
                let verb = if *added { "added" } else { "removed" };
                format!("pane-section-{verb} {section}")
            },
            AppEvent::PaneSectionMoved(section) => format!("pane-section-moved {section}"),
            AppEvent::OmnibarOpened => "omnibar-opened".to_string(),
            AppEvent::OmnibarClosed => "omnibar-closed".to_string(),
            AppEvent::OmnibarCommitted(what) => format!("omnibar-committed {what}"),
            AppEvent::CommandKept { label, kept } => {
                format!("command-{} {label}", if *kept { "kept" } else { "dropped" })
            },
            AppEvent::LayoutReseeded => "layout-reseeded".to_string(),
            AppEvent::ContentState { node, state } => format!("content {node} {state}"),
            AppEvent::PaneSummoned(kind) => format!("pane-summoned {kind}"),
            AppEvent::PaneClosed => "pane-closed".to_string(),
            AppEvent::WorkbenchTileOpened(url) => format!("workbench-opened {url}"),
            AppEvent::WorkbenchTileClosed => "workbench-closed".to_string(),
            AppEvent::WorkbenchStacked => "workbench-stacked".to_string(),
            AppEvent::WorkbenchSplit => "workbench-split".to_string(),
            AppEvent::InteractionMissed { what, target } => {
                format!("interaction-missed {what} {target}")
            },
            AppEvent::AffordanceUnavailable { what, target } => {
                format!("affordance-unavailable {what} {target}")
            },
            AppEvent::ClipPackageExported { clip, path } => {
                format!("clip-package-exported {clip} {path}")
            },
        }
    }
}
