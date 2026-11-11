// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! A signed component pack, reviewed once, answering the pane's adapter calls natively.
//! The component is the fixture guest block-host builds and exposes under its `test-support`
//! feature (engine split D121); no path points into another repository.

use eidetic::pack::{PackManifest, PackPart, PackPartRole, sign_pack};
use eidetic::{ManifestId, TrustEnvelope};
use block_adapter::{
    AdapterError, BlockAdapter, BlockInput, CommandKind, RunResult, ViewAsk,
};
use graphshell::block_editing::adapter::Registry;
use graphshell::capsule_applet::{BLOCK_ADAPTER, CapsuleDisclosure, CapsuleMount, SignedPack};
use block_adapter::GuestEngine;
use graphshell_blocks::attach_reviewed;
use personae::{IdentityProvider, InMemoryProvider};
use std::collections::BTreeMap;

fn component() -> Vec<u8> {
    block_host::test_support::fixture_guest_wasm().unwrap_or_else(|err| panic!("{err}"))
}

fn pack(component: &[u8], asks: Vec<String>) -> Vec<u8> {
    let author = InMemoryProvider::from_seed([7; 32]);
    let manifest = PackManifest {
        name: "fixture-adapter".into(),
        version: "1".into(),
        author: author
            .master_public_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        requested_scopes: asks,
        parts: vec![PackPart {
            name: "part".into(),
            role: PackPartRole::WasmComponent,
            blob: ManifestId::of_blob(component),
            bytes: component.len() as u64,
        }],
    };
    let mut trust = TrustEnvelope::self_asserted();
    trust.signatures.push(sign_pack(&manifest, author.master_keypair()));
    serde_json::to_vec(&SignedPack { manifest, trust }).unwrap()
}

fn hash(bytes: &[u8]) -> String {
    ManifestId::of_blob(bytes).0.to_hex()
}

fn review(component: &[u8], approve: bool) -> CapsuleMount {
    let bytes = pack(component, vec![BLOCK_ADAPTER.into()]);
    let disclosure = CapsuleDisclosure {
        moot: "moot:fixture".into(),
        collection: "collection:adapters".into(),
        revision: "frontier".into(),
        capsules: vec![],
    };
    let mut mount = CapsuleMount::review(&bytes, &hash(&bytes), component, disclosure).unwrap();
    if approve {
        mount.approve(vec![BLOCK_ADAPTER.into()]).unwrap();
    }
    mount
}

fn block(selected: Vec<u32>) -> BlockInput {
    BlockInput {
        class: "fixture".into(),
        body: "one\ntwo\n".into(),
        attrs: BTreeMap::from([("class".into(), "fixture".into())]),
        revision: "r1".into(),
        selected,
        pattern: "xx".into(),
    }
}

#[test]
fn an_unapproved_or_substituted_component_is_not_attached() {
    let engine = GuestEngine::new().unwrap();
    let bytes = component();
    let unapproved = review(&bytes, false);
    assert!(attach_reviewed(&engine, &unapproved, &bytes).is_err(), "review grants nothing");
    let approved = review(&bytes, true);
    let mut other = bytes.clone();
    other.push(0);
    assert!(attach_reviewed(&engine, &approved, &other).is_err(), "not the reviewed bytes");
    assert!(attach_reviewed(&engine, &approved, &bytes).is_ok());
}

#[test]
fn a_reviewed_guest_registers_by_class_and_answers_every_call() {
    let engine = GuestEngine::new().unwrap();
    let bytes = component();
    let mount = review(&bytes, true);
    let adapter = attach_reviewed(&engine, &mount, &bytes).unwrap();
    assert_eq!(adapter.claim().class, "fixture");
    assert_eq!(adapter.claim().pattern.as_ref().unwrap().default, "xx");
    assert_eq!(adapter.claim().commands[0].kind, CommandKind::Choice);

    let mut registry = Registry::with_builtins();
    registry.register(Box::new(adapter)).unwrap();
    assert_eq!(registry.classes(), ["metre", "fixture"]);
    let adapter = registry.get("fixture").unwrap();

    let rows = adapter.rows(&block(vec![])).unwrap();
    assert_eq!(rows.len(), 2);
    let changes = adapter.choices(&block(vec![0, 1]), "mark").unwrap();
    assert_eq!(changes.len(), 2);
    let attrs = adapter.apply(&block(vec![0, 1]), "mark", &changes).unwrap();
    assert_eq!(attrs["fixture"], "marked:2");
    assert_eq!(attrs["class"], "fixture", "the whole map comes back");
    assert_eq!(
        adapter.run(&block(vec![]), "echo", "e").unwrap(),
        RunResult::Text("8:e".into())
    );
    assert_eq!(
        adapter.choices(&block(vec![]), "nope"),
        Err(AdapterError::Refused("Unknown command nope".into()))
    );
    assert!(registry.register(Box::new(
        attach_reviewed(&engine, &mount, &bytes).unwrap()
    )).is_err(), "one adapter per class");
}

#[test]
fn a_guest_view_decodes_into_regions_and_a_painted_sheet() {
    let engine = GuestEngine::new().unwrap();
    let bytes = component();
    let adapter = attach_reviewed(&engine, &review(&bytes, true), &bytes).unwrap();
    let view = adapter
        .view(&block(vec![]), &ViewAsk { mode: "plain".into(), width: 200.0, selected: Some((0, 1)) })
        .unwrap()
        .unwrap();
    assert_eq!(view.description, "fixture view plain");
    let regions = view.regions();
    assert_eq!(regions.len(), 2);
    assert_eq!(regions[0].label, "Event 1 of fixture");
    assert_eq!((regions[1].bar, regions[1].event), (0, 1));
    let leaf = graphshell::block_editing::sheet_leaf::SheetLeaf::new(
        view.sheet.clone(),
        graphshell::block_editing::sheet_leaf::SheetTheme {
            ink: graphshell::block_editing::sheet_leaf::ColorF { r: 0.1, g: 0.1, b: 0.1, a: 1.0 },
            accent: graphshell::block_editing::sheet_leaf::ColorF { r: 0.1, g: 0.4, b: 0.9, a: 1.0 },
            muted: graphshell::block_editing::sheet_leaf::ColorF { r: 0.5, g: 0.5, b: 0.5, a: 1.0 },
        },
    )
    .unwrap();
    assert_eq!(leaf.texts()[0].text, "fixture");
}

#[test]
fn a_spinning_guest_turn_is_refused_and_does_not_hang_the_pane() {
    let engine = GuestEngine::new().unwrap();
    let bytes = component();
    let adapter = attach_reviewed(&engine, &review(&bytes, true), &bytes).unwrap();
    let started = std::time::Instant::now();
    let answer = adapter.run(&block(vec![]), "spin", "");
    assert!(matches!(answer, Err(AdapterError::Refused(text)) if text.contains("stopped")));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

#[test]
fn a_returned_attribute_map_changes_only_that_block_in_one_undo_step() {
    use graphshell::block_editing::{Document, Preview, wrap};
    let engine = GuestEngine::new().unwrap();
    let bytes = component();
    let adapter = attach_reviewed(&engine, &review(&bytes, true), &bytes).unwrap();
    let attrs = BTreeMap::from([
        ("class".to_owned(), "fixture".to_owned()),
        ("id".to_owned(), "fixture-test".to_owned()),
    ]);
    let source = format!("# Chart\n\n{}", wrap(&attrs, "one\ntwo\n"));
    let mut doc = Document::new("conformance", &source);
    let before = doc.editor.source().to_owned();
    let div = doc.blocks().into_iter().next().expect("one block");
    let input = BlockInput {
        class: "fixture".into(),
        body: before[div.body.clone()].to_owned(),
        attrs: div.attributes.clone(),
        revision: "r1".into(),
        selected: vec![0, 1],
        pattern: "xx".into(),
    };
    let changes = adapter.choices(&input, "mark").unwrap();
    let returned = adapter.apply(&input, "mark", &changes).unwrap();
    let differing: Vec<_> = returned
        .iter()
        .filter(|(k, v)| div.attributes.get(*k) != Some(*v))
        .map(|(k, _)| k.as_str())
        .collect();
    assert_eq!(differing, ["fixture"], "only the source attribute changes");
    let preview = Preview::new(doc.editor.source(), div, changes).unwrap();
    doc.apply(&preview, returned).unwrap();
    assert_ne!(doc.editor.source(), before);
    assert!(doc.editor.source().starts_with("# Chart\n\n"), "text outside the block is untouched");
    doc.editor.apply(cambium::TextCommand::Undo);
    assert_eq!(doc.editor.source(), before, "one undo restores the document byte for byte");
}
