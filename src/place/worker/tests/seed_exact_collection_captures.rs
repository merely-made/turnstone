// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Seed only the local Fleece records and their read-only projection.
/// This deliberately creates no Moot or group state, so a live receiver
/// can retain its own captures while its governed history arrives from a
/// peer over the normal admission and lane paths.
pub(crate) fn seed_exact_collection_captures(
    directory: &Path,
) -> (PlaceWorkerSettings, [u8; 32], [u8; 32]) {
    let capture_directory = crate::trail_memory::memory_dir(directory);
    let document = extract_document(&StaticDocument::parse(
        "<main><p>Exact collection selection keeps this field note.</p></main>",
    ));
    let anchor = anchor_for_range(
        &document.page.text,
        TextPositionSelector {
            start: 0,
            end: document.page.text.chars().count() as u64,
        },
        document.contract.quote_context,
    )
    .unwrap();
    let record = FleeceAnnotationRecord::from_fleece(
        CaptureIdentity::new(
            "https://collection.test/field-note",
            eidetic::Hash::of(b"exact-collection-source"),
        )
        .unwrap(),
        &document,
        &anchor,
    )
    .unwrap();
    let other_document = extract_document(&StaticDocument::parse(
        "<main><p>An unselected capture remains in the Moot.</p></main>",
    ));
    let other_anchor = anchor_for_range(
        &other_document.page.text,
        TextPositionSelector {
            start: 0,
            end: other_document.page.text.chars().count() as u64,
        },
        other_document.contract.quote_context,
    )
    .unwrap();
    let other_record = FleeceAnnotationRecord::from_fleece(
        CaptureIdentity::new(
            "https://collection.test/unselected",
            eidetic::Hash::of(b"unselected-collection-source"),
        )
        .unwrap(),
        &other_document,
        &other_anchor,
    )
    .unwrap();
    let (manifest, other_manifest) = {
        let mut store = FjallStore::open(&capture_directory).unwrap();
        pollster::block_on(fleece::bootstrap_fleece_annotation_schema(&mut store)).unwrap();
        (
            pollster::block_on(fleece::save_fleece_annotation(&mut store, &record, 20)).unwrap(),
            pollster::block_on(fleece::save_fleece_annotation(
                &mut store,
                &other_record,
                21,
            ))
            .unwrap(),
        )
    };

    let settings = settings();
    // These records belong to an admitted local source owner. The production
    // cache remains closed until custody is explicitly bound.
    let custody = crate::trail_memory::custody_test_fixture::custody();
    assert!(settings.capture_library.bind_custody(&custody));
    {
        let mut store = FjallStore::open(&capture_directory).unwrap();
        crate::place::captured_collection::refresh_local_capture_library(
            &mut store,
            &capture_directory,
            &settings.capture_library,
        )
        .unwrap();
    }
    (
        settings,
        *manifest.0.as_bytes(),
        *other_manifest.0.as_bytes(),
    )
}
