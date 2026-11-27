// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashSet;

use super::*;

#[test]
fn built_in_ids_and_schemas_are_unique_and_namespaced() {
    let mut ids = HashSet::new();
    let mut configs = HashSet::new();
    let mut views = HashSet::new();
    for pane in BUILTIN_PANES {
        assert!(pane.id.starts_with("turnstone."));
        assert!(pane.config_schema.starts_with("turnstone."));
        assert!(pane.view_schema.starts_with("turnstone."));
        assert!(ids.insert(pane.id.clone()));
        assert!(configs.insert(pane.config_schema));
        assert!(views.insert(pane.view_schema));
    }
}

#[test]
fn publishing_workflows_are_typed_registrations() {
    let publishing = pane_definition(kind::PUBLISHING).unwrap();
    let shared = pane_definition(kind::SHARED_KNOT).unwrap();
    assert_eq!(publishing.renderer, PaneRenderer::Publishing);
    assert_eq!(shared.renderer, PaneRenderer::SharedKnot);
    assert_eq!(publishing.multiplicity, PaneMultiplicity::Many);
    assert_eq!(shared.multiplicity, PaneMultiplicity::Many);
    assert_eq!(
        shared.source_shape.fixed,
        &[FixedSourceKind::External(source_schema::SHARE_TICKET)]
    );
}

#[test]
fn registration_is_the_legacy_content_factory() {
    assert_eq!(
        legacy_pane_content(&PaneKindId::new(kind::PUBLISHING)),
        Some(PaneContent::Registered(PaneKindId::new(kind::PUBLISHING)))
    );
    assert_eq!(
        legacy_pane_content(&PaneKindId::new(kind::SHARED_KNOT)),
        Some(PaneContent::Registered(PaneKindId::new(kind::SHARED_KNOT)))
    );
    assert_eq!(legacy_pane_content(&PaneKindId::new(kind::TILE)), None);
}

#[test]
fn retired_system_and_placeholder_are_not_registered() {
    assert!(pane_definition("turnstone.system").is_none());
    assert!(pane_definition("__placeholder__").is_none());
}

#[test]
fn source_shapes_validate_fixed_and_following_sources() {
    let roster = pane_definition(kind::ROSTER).unwrap();
    assert!(
        roster
            .source_shape
            .accepts(&PaneSource::FromContext(SourceSelector::Graph))
    );
    assert!(
        !roster
            .source_shape
            .accepts(&PaneSource::FromContext(SourceSelector::Member))
    );

    let shared = pane_definition(kind::SHARED_KNOT).unwrap();
    let source = |schema| {
        PaneSource::Fixed(SourceRef::External {
            schema: super::super::SourceSchemaId::new(schema),
            payload: super::super::SerializedSource {
                version: 1,
                payload: serde_json::Value::Null,
            },
        })
    };
    assert!(
        shared
            .source_shape
            .accepts(&source(source_schema::SHARE_TICKET))
    );
    assert!(
        !shared
            .source_shape
            .accepts(&source("turnstone.some-other-ticket"))
    );
}

fn runtime(id: &str, title: &str) -> PaneDefinition {
    PaneDefinition::runtime_contributed(
        id.into(),
        title.into(),
        format!("Open {title} pane"),
        PaneSourceShape::fixed(&[]),
        "turnstone.test.config",
        "turnstone.test.view",
        crate::mod_viewer::source(id),
    )
}

#[test]
fn a_runtime_kind_is_added_found_listed_and_removed() {
    let before = pane_palette_entries().len();
    assert!(pane_definition("mod:a").is_none());
    define_runtime_pane(runtime("mod:b", "Beta"));
    define_runtime_pane(runtime("mod:a", "Alpha"));
    let alpha = pane_definition("mod:a").unwrap();
    assert_eq!(
        (&*alpha.display_name, alpha.renderer, alpha.multiplicity),
        (
            "Alpha",
            PaneRenderer::Contributed,
            PaneMultiplicity::PerSpaceAndSource
        )
    );
    assert_eq!(alpha.default_source, Some(crate::mod_viewer::source("mod:a")));
    // The built-ins keep their own table, and a runtime kind never shadows one.
    assert!(matches!(
        pane_definition(kind::SETTINGS),
        Some(DefinitionRef::Builtin(_))
    ));
    // Palette entries: the built-ins, then the runtime kinds by label.
    let entries = pane_palette_entries();
    assert_eq!(entries.len(), before + 2);
    let labels: Vec<&str> = entries[before..].iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels, ["Open Alpha pane", "Open Beta pane"]);
    assert!(
        builtin_palette_entries()
            .iter()
            .all(|(_, kind)| !kind.as_str().starts_with("mod:"))
    );
    // The same id replaces: a retitle.
    define_runtime_pane(runtime("mod:a", "Alef"));
    assert_eq!(runtime_pane_ids(), ["mod:b", "mod:a"]);
    assert_eq!(&*pane_definition("mod:a").unwrap().display_name, "Alef");
    assert!(remove_runtime_pane("mod:a") && !remove_runtime_pane("mod:a"));
    assert!(pane_definition("mod:a").is_none());
    assert!(remove_runtime_pane("mod:b"));
    assert_eq!(pane_palette_entries().len(), before);
}
