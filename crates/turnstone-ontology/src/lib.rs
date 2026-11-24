// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The app's selected subject packs and the reading actions understood by Turnstone.

use mere_ontology::{CITATION, Profile, WEB_ANNOTATION};
use std::path::Path;
use std::sync::OnceLock;

mod reader;
pub use reader::{
    MAX_DOCUMENT_BYTES, Passage, Reading, open_annotation, passages, passages_from_file, read,
};

pub fn load_profile(directory: Option<&Path>) -> Result<Profile, String> {
    mere_ontology::load(
        include_str!("../packs/lens.toml"),
        &[
            WEB_ANNOTATION,
            CITATION,
            include_str!("../packs/provenance.toml"),
        ],
        directory,
    )
}

/// An explicit local override applies to this process, including its GUI and file-open command.
pub fn selected_profile() -> Result<&'static Profile, &'static str> {
    static PROFILE: OnceLock<Result<Profile, String>> = OnceLock::new();
    PROFILE
        .get_or_init(|| {
            let directory =
                std::env::var_os("TURNSTONE_ONTOLOGY_DIR").map(std::path::PathBuf::from);
            load_profile(directory.as_deref())
        })
        .as_ref()
        .map_err(String::as_str)
}

/// Project explicit RDF media and JSON responses. Plain text and arbitrary XML
/// retain their usual readers. The effective response address resolves relative IRIs.
pub fn inline_reading(content_type: Option<&str>, bytes: &[u8], base: Option<&str>) -> Reading {
    let Some(mime) = content_type.and_then(|mime| mime.split(';').next()) else {
        return Reading::default();
    };
    let mime = mime.trim().to_ascii_lowercase();
    if bytes.len() > MAX_DOCUMENT_BYTES
        || !matches!(
            mime.as_str(),
            "application/json"
                | "application/ld+json"
                | "text/turtle"
                | "application/n-triples"
                | "application/n-quads"
                | "application/trig"
                | "application/rdf+xml"
        )
    {
        return Reading::default();
    }
    let Some(format) = mere::formats::RdfFormat::from_media_type(&mime) else {
        return Reading::default();
    };
    selected_profile()
        .ok()
        .and_then(|profile| reader::read_format(profile, bytes, format, base).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
