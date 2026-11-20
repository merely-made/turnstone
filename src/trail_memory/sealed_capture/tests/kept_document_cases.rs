// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn kept_document_reopens_raw_and_decoded_html_after_redirect() {
    let root = tempfile::tempdir().unwrap();
    let sealer = TestSealer(AtomicBool::new(true));
    let document = FetchedDocument {
        bytes: b"<p>Caf\xe9</p><script type='application/ld+json'>{}</script>".to_vec(),
        body: "<p>Café</p><script type='application/ld+json'>{}</script>".into(),
        effective_url: Some("https://example.test/destination".into()),
        content_type: Some("text/html; charset=windows-1252".into()),
        acquired_at_ms: 1,
    };
    let request = CaptureRequest {
        id: uuid::Uuid::new_v4(),
        session: crate::panes::SessionId::new(),
        graph: crate::panes::GraphId::new(),
        node: uuid::Uuid::new_v4(),
        url: "https://example.test/start".into(),
        source: Some(SourceIdentity::of(&document)),
        keep: true,
    };
    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    let stored = capture_request(&mut store, &sealer, &request, Some(&document)).unwrap();
    acknowledge(&mut store, &sealer, &request).unwrap();
    drop(store);

    let mut store = muniment::FjallStore::open(root.path()).unwrap();
    let reopened = read_kept_document(&mut store, &sealer, &request, &stored).unwrap();
    assert_eq!(reopened.bytes, document.bytes);
    assert_eq!(reopened.body, document.body);
    assert_eq!(SourceIdentity::of(&reopened), SourceIdentity::of(&document));

    let mut changed = request.clone();
    changed.source.as_mut().unwrap().effective_url = Some(request.url.clone());
    assert!(read_kept_document(&mut store, &sealer, &changed, &stored).is_err());

    let mut unkept = request.clone();
    unkept.keep = false;
    assert!(read_kept_document(&mut store, &sealer, &unkept, &stored).is_err());

    sealer.0.store(false, Ordering::SeqCst);
    assert!(read_kept_document(&mut store, &sealer, &request, &stored).is_err());
}
