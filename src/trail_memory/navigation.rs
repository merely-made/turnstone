// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Map a drained [`AppEvent`](crate::observe::AppEvent) onto a traversal,
/// when it is one at all. `AddressOpened` reads as a typed address: the
/// omnibar, a trail row, and a grid click all lower through
/// `Action::OpenAddress`, and the event does not carry which.
pub fn navigation(event: &crate::observe::AppEvent) -> Option<(String, TraceTransition)> {
    use crate::observe::AppEvent;
    match event {
        AppEvent::AddressOpened(url) => Some((url.clone(), TraceTransition::UrlTyped)),
        // The engine callback does not distinguish a link activation from a
        // redirect. Preserve the traversal without manufacturing a cause.
        AppEvent::ContentNavigated { url, .. } => Some((url.clone(), TraceTransition::Unknown)),
        AppEvent::NavigatedBack(url) => Some((url.clone(), TraceTransition::Back)),
        AppEvent::NavigatedForward(url) => Some((url.clone(), TraceTransition::Forward)),
        AppEvent::Reloaded(url) => Some((url.clone(), TraceTransition::Reload)),
        _ => None,
    }
}
