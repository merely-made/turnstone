// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The scope an app event answers to: `app/<its kebab name>`.
///
/// The name is `describe`'s first token, which is that method's shape for all
/// 52 variants. Read from it rather than duplicated into a second 52-arm match
/// that could disagree with the one the transcript already shows the user.
pub fn app_event_scope(event: &AppEvent) -> Option<ScopePath> {
    let described = event.describe();
    let name = described.split_whitespace().next()?;
    ScopePath::parse(&format!("{APP_SCOPE_ROOT}/{name}")).ok()
}
