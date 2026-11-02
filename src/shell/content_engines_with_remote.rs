// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn content_engines_with_remote(remote: RemoteFetcher) -> SessionRegistry<Scene> {
    let mut engines =
        pelt_core::native::sessions_with_fetcher(LocalFetcher.with_fallback(remote.clone()));
    engines.register(Box::new(crate::nomadnet::MicronSessionEngine::new()));
    engines.register(Box::new(saved_sources::SavedSourceSessionEngine));
    for engine_id in SMOLWEB_SESSION_ENGINE_IDS {
        engines.register(Box::new(
            SmolwebSessionEngine::new(
                *engine_id,
                LocalFetcher.with_fallback(remote.clone()),
                SmolwebTheme::default(),
            )
            .with_inline_media(smolweb_inline_media_policy()),
        ));
    }
    engines
}
