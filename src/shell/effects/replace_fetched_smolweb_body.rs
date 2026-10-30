// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

/// A retained response keeps its native media type when the live document is
/// replaced, just as it does when the initial controller is prepared.
pub(super) fn replace_fetched_smolweb_body(
    session: &mut mere_document_lanes::SmolwebDocumentSession,
    url: &str,
    document: &crate::content::FetchedDocument,
) {
    let mut input = inker::EngineInput::new(url, document.body.clone());
    if let Some(content_type) = &document.content_type {
        input = input.with_content_type(content_type.clone());
    }
    session.replace_input(&input);
}
