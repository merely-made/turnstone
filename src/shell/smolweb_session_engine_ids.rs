// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// Engine-native smolweb lanes Turnstone can route to through Inker's default
/// policy. Spartan shares Gemini's gemtext lane; feed is selected by response
/// media type, while the other ids correspond directly to protocol schemes.
pub(super) const SMOLWEB_SESSION_ENGINE_IDS: &[&str] = &[
    inker::routing::ENGINE_NEMATIC_GEMTEXT,
    inker::routing::ENGINE_NEMATIC_GOPHER,
    inker::routing::ENGINE_NEMATIC_FINGER,
    inker::routing::ENGINE_NEMATIC_SCROLL,
    inker::routing::ENGINE_NEMATIC_NEX,
    inker::routing::ENGINE_NEMATIC_GUPPY,
    inker::routing::ENGINE_NEMATIC_TITAN,
    inker::routing::ENGINE_NEMATIC_FEED,
];
