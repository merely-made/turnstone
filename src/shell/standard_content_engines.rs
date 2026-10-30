// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The content lanes that are always available in an ordinary Turnstone
/// build. Keeping their construction together makes route availability an
/// inspectable fact instead of an assumption split across shell setup.
pub(crate) fn standard_content_engines() -> SessionRegistry<Scene> {
    content_engines_with_remote(RemoteFetcher::shared())
}
