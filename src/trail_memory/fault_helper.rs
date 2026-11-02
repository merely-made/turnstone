// STAGED SOURCE, NOT IN THE BUILD. Integrate as a child of m5_inspection only
// after the runner containment and source-freeze review described in report.md.
// One ignored test may invoke inject(); it must run after the intact processes
// have exited, inside the fault-only mount namespace.
use super::*;
use eidetic::Store;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: String,
    run_id: uuid::Uuid,
    a1_admission: crate::saved_sources::Admission,
    a1_target: crate::saved_sources::Target,
    a1_content: crate::saved_sources::Content,
    a2_admission: crate::saved_sources::Admission,
    a2_target: crate::saved_sources::Target,
    a2_content: crate::saved_sources::Content,
    all_admissions: Vec<crate::saved_sources::Admission>,
    sealed_capture_receipts: ReceiptBaseline,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptBaseline { schema: String, count: usize, blake3: String }

fn verify_receipt_baseline(receipts: &[crate::source_capture::DepositReceipt], baseline: &ReceiptBaseline) {
    assert_eq!(baseline.schema, "turnstone.m5-sealed-capture-receipts/v1");
    assert_eq!(receipts.len(), baseline.count, "receipt inventory count differs from intact inspection");
    let canonical = serde_json::to_vec(receipts).expect("receipt inventory serialization");
    assert_eq!(blake3::hash(&canonical).to_hex().to_string(), baseline.blake3,
        "receipt inventory differs from intact inspection");
}

// A session's sealed capture owner is a distinct Fjall partition/root. Its
// complete receipt inventory includes saved, pending, failed and detached
// capture requests. The target's own decoded body may share its raw bytes.
fn exclusively_referenced(receipts: &[crate::source_capture::DepositReceipt],
                          a: &crate::saved_sources::Admission, raw: &str) -> bool {
    let own: Vec<_> = receipts.iter().filter(|r| r.saved && r.request.session == a.key.session
        && r.request.graph == a.key.graph && r.request.node == a.key.node
        && r.request.source.as_ref().is_some_and(|s| s.hash == raw)
        && r.stored.as_ref().is_some_and(|s| s.annotation_manifest == a.key.annotation_manifest
            && s.raw_manifest == raw)).collect();
    if own.len() != 1 { return false; }
    let owner_id = own[0].request.id;
    receipts.iter().all(|r| {
        if r.request.id == owner_id { return true; }
        let Some(source) = r.request.source.as_ref() else { return false; };
        source.hash != raw && source.body_hash != raw && r.stored.as_ref().is_none_or(|s|
            s.raw_manifest != raw && s.annotation_manifest.strip_prefix("sealed-v1:").unwrap_or(&s.annotation_manifest) != raw)
    })
}

#[cfg(test)]
mod sharing_tests {
    use super::*;

    #[test]
    fn model_free_missing_fault_rejects_saved_and_unsaved_shared_receipts() {
        let session = crate::panes::SessionId::new();
        let graph = crate::panes::GraphId::new();
        let node = uuid::Uuid::new_v4();
        let key = crate::saved_sources::Key { session, graph, node, annotation_manifest:"sealed-v1:annotation".into() };
        let a = crate::saved_sources::Admission { key, occurrence:0, title:String::new(), url:String::new(), error:None };
        let source = crate::source_capture::SourceIdentity { hash:"raw-a".into(), body_hash:"raw-a".into(),
            effective_url:None, content_type:None, acquired_at_ms:1 };
        let request = crate::source_capture::CaptureRequest { id:uuid::Uuid::new_v4(), session, graph, node,
            url:String::new(), source:Some(source), keep:false };
        let saved = crate::source_capture::DepositReceipt { saved:true, request,
            stored:Some(crate::action::StoredSourceDocument { raw_manifest:"raw-a".into(),
                annotation_manifest:"sealed-v1:annotation".into() }) };
        assert!(exclusively_referenced(&[saved.clone()], &a, "raw-a"));
        let mut other = saved.clone();
        other.request.id = uuid::Uuid::new_v4();
        other.request.node = uuid::Uuid::new_v4();
        other.saved = false;
        other.stored = None;
        assert!(!exclusively_referenced(&[saved.clone(), other.clone()], &a, "raw-a"));
        other.request.source.as_mut().unwrap().hash = "different".into();
        assert!(!exclusively_referenced(&[saved.clone(), other.clone()], &a, "raw-a"));
        other.request.source.as_mut().unwrap().body_hash = "different".into();
        other.stored = Some(crate::action::StoredSourceDocument { raw_manifest:"raw-a".into(), annotation_manifest:"sealed-v1:other".into() });
        assert!(!exclusively_referenced(&[saved, other], &a, "raw-a"));
    }
}

fn assert_graph_ref(graph: &mere::kernel::graph::Graph, admission: &crate::saved_sources::Admission) {
    assert!(admission.error.is_none() && admission.key.is_sealed());
    let (_, node) = graph.get_node_by_id(admission.key.node).expect("source node missing");
    let facet = graph.facets().get(&node.id, &chartulary::FacetId::new(
        crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET,
    )).and_then(|v| v.as_array()).expect("source facet missing");
    assert_eq!(facet.get(admission.occurrence).and_then(|v| v.as_str()),
        Some(admission.key.annotation_manifest.as_str()));
}

// These paths are fixed by a separately reviewed mount namespace, not supplied
// by the input JSON. /m5/fault-copy is the ONLY writable profile mount.
pub(super) fn inject() {
    let root = PathBuf::from("/m5/fault-copy");
    assert_eq!(root.canonicalize().unwrap(), root);
    let input: Input = serde_json::from_slice(&std::fs::read("/m5/input/missing-input.json").unwrap()).unwrap();
    assert_eq!(input.schema, "turnstone.m5-missing-input/v1");
    let marker = format!("turnstone-m5-fault-copy/v1\n{}\n", input.run_id);
    assert_eq!(std::fs::read(root.join("m5-fault-copy-marker")).unwrap(), marker.as_bytes());
    let fixture = Path::new("/m5/custody").canonicalize().unwrap();
    assert_eq!(fixture, Path::new("/m5/custody"));
    assert_eq!(std::fs::read(fixture.join("fixture-marker")).unwrap(), b"eidetic-retention-20261019\n");
    let endpoint = Path::new("/m5/custody/app.sock").canonicalize().unwrap();
    assert!(endpoint.starts_with(&fixture));
    let sealer = crate::identity::RootIdentity::Custody(
        djinn::door::custody_identity::CustodyIdentity::connect_at(
            endpoint.to_str().unwrap(), "turnstone").expect("fixture custody"));

    let a = &input.a1_admission;
    let b = &input.a2_admission;
    assert_eq!(a.key, input.a1_target.key);
    assert_eq!(b.key, input.a2_target.key);
    assert!(input.a1_target.selector.is_some(), "A1 must be an exact passage citation");
    assert_eq!(a.key.session, b.key.session);
    assert_eq!(a.key.graph, b.key.graph);
    assert_eq!(a.key.node, b.key.node);
    assert_ne!(a.key.annotation_manifest, b.key.annotation_manifest);
    assert_ne!(input.a1_content.text_hash, input.a2_content.text_hash);
    assert_ne!(input.a1_content.raw_manifest, input.a2_content.raw_manifest,
        "A2 shares the A1 raw payload");
    assert_eq!(input.a1_target.text_hash, input.a1_content.text_hash);
    assert_eq!(input.a2_target.text_hash, input.a2_content.text_hash);
    crate::saved_sources::validate_selector(&input.a1_content, &input.a1_target).unwrap().expect("A1 quote");
    crate::saved_sources::validate_selector(&input.a2_content, &input.a2_target).unwrap();
    let session = *a.key.session.as_uuid();
    let session_dir = root.join("sessions").join(session.to_string());
    let graph = super::strict_inspection_graph(&session_dir).expect("fault copy graph");
    assert_graph_ref(&graph, a);
    assert_graph_ref(&graph, b);
    let mut graph_refs = std::collections::HashSet::new();
    for (_, node) in graph.nodes() {
        if let Some(value) = graph.facets().get(&node.id, &chartulary::FacetId::new(
            crate::source_capture::SOURCE_CAPTURE_REFERENCES_FACET)) {
            for (occurrence, value) in value.as_array().expect("invalid reference series").iter().enumerate() {
                assert!(graph_refs.insert((node.id, occurrence, value.as_str().expect("invalid reference").to_owned())));
            }
        }
    }
    let mut observed_refs = std::collections::HashSet::new();
    for item in &input.all_admissions {
        assert_eq!(item.key.session, a.key.session);
        assert_eq!(item.key.graph, a.key.graph);
        assert_graph_ref(&graph, item);
        assert!(observed_refs.insert((item.key.node, item.occurrence, item.key.annotation_manifest.clone())));
    }
    assert_eq!(graph_refs, observed_refs, "capture inventory incomplete");
    assert!(input.all_admissions.iter().any(|item| item == a));
    assert!(input.all_admissions.iter().any(|item| item == b));
    let sealed = super::super::sealed_memory_dir(&session_dir.join("memory"));
    assert!(sealed.is_dir());
    let mut store = FjallStore::open(&sealed).expect("fault copy store");
    let receipts_before = super::super::sealed_capture::receipts(&mut store, &sealer).expect("capture receipts");
    verify_receipt_baseline(&receipts_before, &input.sealed_capture_receipts);
    assert!(exclusively_referenced(&receipts_before, a, &input.a1_content.raw_manifest),
        "another saved or unsaved capture receipt references A1 bytes, or A1 receipt is ambiguous");
    let receipt_digest = format!("blake3:{}", input.sealed_capture_receipts.blake3);
    assert_eq!(super::super::sealed_capture::content(&mut store, &sealer, a).unwrap(), input.a1_content);
    assert_eq!(super::super::sealed_capture::content(&mut store, &sealer, b).unwrap(), input.a2_content);
    for item in &input.all_admissions {
        let content = super::super::sealed_capture::content(&mut store, &sealer, item).expect("capture inventory unavailable");
        if item != a {
            assert_ne!(content.raw_manifest, input.a1_content.raw_manifest, "another admitted capture shares A1 bytes");
        }
    }
    let raw: eidetic::ManifestId = serde_json::from_value(
        serde_json::Value::String(input.a1_content.raw_manifest.clone())).unwrap();
    let manifest = pollster::block_on(eidetic::manifest::load_manifest(&mut store, raw)).unwrap().unwrap();
    let key = format!("blob:{}", raw.0.to_hex());
    assert_eq!(manifest.id, raw);
    assert_eq!(manifest.content_hash, raw.0);
    assert_eq!(manifest.sources.len(), 1);
    assert!(matches!(&manifest.sources[0], eidetic::BlobSource::Local { key: held } if held == &key));
    assert!(eidetic::seal::seal_marker(&manifest).unwrap().is_some());
    let before = pollster::block_on(store.get(&key)).unwrap().expect("A1 bytes absent before fault");
    let ciphertext_hash = eidetic::Hash::of(&before).to_string();
    // Owner API only. Keep the manifest, graph admission and receipt intact.
    pollster::block_on(store.delete(&key)).expect("owner deletion");
    store.sync_all().expect("durable fault");
    drop(store);
    let mut store = FjallStore::open(&sealed).unwrap();
    assert!(pollster::block_on(store.get(&key)).unwrap().is_none());
    assert_eq!(pollster::block_on(eidetic::manifest::load_manifest(&mut store, raw)).unwrap().unwrap(), manifest);
    assert_eq!(super::super::sealed_capture::receipts(&mut store, &sealer).expect("capture receipts after deletion"), receipts_before,
        "capture receipts changed");
    assert!(super::super::sealed_capture::content(&mut store, &sealer, a).is_err());
    assert_eq!(super::super::sealed_capture::content(&mut store, &sealer, b).unwrap(), input.a2_content);
    let receipt = serde_json::json!({"schema":"turnstone.m5-missing-fault/v1", "run_id":input.run_id,
        "a1_target":input.a1_target, "a2_target":input.a2_target,
        "a1_raw_manifest":input.a1_content.raw_manifest, "a2_raw_manifest":input.a2_content.raw_manifest,
        "deleted_ciphertext_hash":ciphertext_hash, "manifest_preserved":true,
        "capture_receipts_hash":receipt_digest, "capture_receipts_preserved":true,
        "a1_raw_absent_after_reopen":true, "a1_owner_refused":true, "a2_owner_unchanged":true});
    // create_new ensures a failed or repeated attempt cannot overwrite evidence.
    use std::io::Write;
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true)
        .open("/m5/output/fault.json").expect("fresh output");
    out.write_all(&serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    out.sync_all().unwrap();
}

// A second ignored test runs this in a fresh process after inject exits and
// before the GUI opens. It checks owner state, including the unchanged receipt
// inventory, rather than trusting the injector's success status alone.
pub(super) fn verify_preservation() {
    let root = PathBuf::from("/m5/fault-copy");
    assert_eq!(root.canonicalize().unwrap(), root);
    let input: Input = serde_json::from_slice(&std::fs::read("/m5/input/missing-input.json").unwrap()).unwrap();
    assert_eq!(input.schema, "turnstone.m5-missing-input/v1");
    assert_eq!(std::fs::read(root.join("m5-fault-copy-marker")).unwrap(),
        format!("turnstone-m5-fault-copy/v1\n{}\n", input.run_id).as_bytes());
    let fault: serde_json::Value = serde_json::from_slice(&std::fs::read("/m5/output/fault.json").unwrap()).unwrap();
    assert_eq!(fault["schema"], "turnstone.m5-missing-fault/v1");
    assert_eq!(fault["run_id"], input.run_id.to_string());
    assert_eq!(fault["a1_target"], serde_json::to_value(&input.a1_target).unwrap());
    assert_eq!(fault["a2_target"], serde_json::to_value(&input.a2_target).unwrap());
    let fixture = Path::new("/m5/custody").canonicalize().unwrap();
    assert_eq!(fixture, Path::new("/m5/custody"));
    assert_eq!(std::fs::read(fixture.join("fixture-marker")).unwrap(), b"eidetic-retention-20261019\n");
    let endpoint = fixture.join("app.sock").canonicalize().unwrap();
    assert!(endpoint.starts_with(&fixture));
    let sealer = crate::identity::RootIdentity::Custody(
        djinn::door::custody_identity::CustodyIdentity::connect_at(endpoint.to_str().unwrap(), "turnstone").unwrap());
    let a = &input.a1_admission;
    let session = *a.key.session.as_uuid();
    let session_dir = root.join("sessions").join(session.to_string());
    let graph = super::strict_inspection_graph(&session_dir).unwrap();
    assert_graph_ref(&graph, a);
    assert_graph_ref(&graph, &input.a2_admission);
    let sealed = super::super::sealed_memory_dir(&session_dir.join("memory"));
    assert!(sealed.is_dir());
    let mut store = FjallStore::open(sealed).unwrap();
    let receipts = super::super::sealed_capture::receipts(&mut store, &sealer).unwrap();
    verify_receipt_baseline(&receipts, &input.sealed_capture_receipts);
    assert!(exclusively_referenced(&receipts, a, &input.a1_content.raw_manifest));
    let digest = format!("blake3:{}", input.sealed_capture_receipts.blake3);
    assert_eq!(fault["capture_receipts_hash"], digest);
    assert_eq!(receipts.iter().filter(|r| r.saved && r.request.session == a.key.session
        && r.request.graph == a.key.graph && r.request.node == a.key.node
        && r.stored.as_ref().is_some_and(|s| s.annotation_manifest == a.key.annotation_manifest
            && s.raw_manifest == input.a1_content.raw_manifest)).count(), 1);
    let raw: eidetic::ManifestId = serde_json::from_value(
        serde_json::Value::String(input.a1_content.raw_manifest.clone())).unwrap();
    let manifest = pollster::block_on(eidetic::manifest::load_manifest(&mut store, raw)).unwrap().unwrap();
    let key = format!("blob:{}", raw.0.to_hex());
    assert_eq!(manifest.id, raw);
    assert_eq!(manifest.content_hash, raw.0);
    assert_eq!(manifest.sources.len(), 1);
    assert!(matches!(&manifest.sources[0], eidetic::BlobSource::Local { key: held } if held == &key));
    assert!(eidetic::seal::seal_marker(&manifest).unwrap().is_some());
    assert!(pollster::block_on(store.get(&key)).unwrap().is_none());
    assert!(super::super::sealed_capture::content(&mut store, &sealer, a).is_err());
    assert_eq!(super::super::sealed_capture::content(&mut store, &sealer, &input.a2_admission).unwrap(), input.a2_content);
    let receipt = serde_json::json!({"schema":"turnstone.m5-missing-postcheck/v1", "run_id":input.run_id,
        "capture_receipts_hash":digest, "capture_receipts_preserved":true,
        "a1_raw_absent":true, "a1_owner_refused":true, "a2_owner_unchanged":true});
    use std::io::Write;
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true)
        .open("/m5/output/fault-postcheck.json").unwrap();
    out.write_all(&serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    out.sync_all().unwrap();
}
