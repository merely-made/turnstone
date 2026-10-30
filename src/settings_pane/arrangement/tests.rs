use std::collections::{BTreeMap, BTreeSet};

use crate::settings_pane::APPLICATION_REFERENCE;
use cambium::composition::{
    ComponentDescriptor, ComponentKey, ComponentKind, Composition, CompositionEdit,
    CompositionLimits, CompositionTransaction, ContainerKey, ContainerKind, PlacementNode,
};
use mere_surface_api::settings::{
    SettingControl, SettingMovement, SettingMutability, SettingScope, SettingSecurity, SettingValue,
};
use serde_json::{Value, json};

use super::*;
use crate::panes::{PaneConfig, PaneConfigSchemaId};
use crate::panes::{PaneViewSchemaId, PaneViewState};

fn spec(id: &str) -> SettingSpec {
    SettingSpec {
        id: id.into(),
        label: id.into(),
        scope: SettingScope::Application,
        movement: SettingMovement::LocalOnly,
        mutability: SettingMutability::Live,
        security: SettingSecurity::Ordinary,
        control: SettingControl::Toggle,
        value: SettingValue::Boolean(false),
    }
}
fn sources(ref_id: &str, specs: &[SettingSpec]) -> Sources {
    BTreeMap::from([(
        ref_id.into(),
        specs
            .iter()
            .cloned()
            .map(|spec| (spec.id.clone(), spec))
            .collect(),
    )])
}
fn source_specs() -> Vec<SettingSpec> {
    vec![
        spec("theme.choice"),
        spec("shell.zoom"),
        spec("behavior.cascade"),
    ]
}
fn key(value: &str) -> ComponentKey {
    ComponentKey::from(value)
}
fn root() -> ContainerKey {
    ContainerKey {
        kind: ContainerKind::Root,
        key: String::new(),
    }
}
fn group(key: &str) -> ContainerKey {
    ContainerKey {
        kind: ContainerKind::Group,
        key: key.into(),
    }
}
fn row(instance: &str, reference: &str, setting_id: &str) -> ComponentDescriptor {
    ComponentDescriptor {
        key: key(instance),
        kind: ComponentKind(SETTING_KIND.into()),
        config: Value::Null,
        binding: binding(reference, setting_id),
    }
}
fn model(rows: &[ComponentDescriptor]) -> Composition {
    Composition {
        components: rows.to_vec(),
        placement: rows
            .iter()
            .map(|row| PlacementNode::Component {
                key: row.key.clone(),
            })
            .collect(),
        unplaced: Vec::new(),
    }
}
fn app_arrangement(specs: &[SettingSpec]) -> SettingsArrangement {
    SettingsArrangement::from_application_specs(specs).unwrap()
}
fn commit(
    arrangement: &mut SettingsArrangement,
    operation: ArrangementOperation,
) -> ArrangementEffect {
    let prepared = arrangement.preview(operation).unwrap();
    arrangement.commit(prepared).unwrap()
}

#[test]
fn deleting_every_row_then_adding_a_registered_setting_allocates_a_fresh_instance() {
    let specs = vec![spec("theme.choice")];
    let mut arrangement = app_arrangement(&specs);
    let old = arrangement.accepted().components[0].clone();
    let approval = DeletionApproval::after_confirmation(old.key.clone(), true, true, true).unwrap();
    commit(
        &mut arrangement,
        ArrangementOperation::Delete {
            key: old.key.clone(),
            approval,
        },
    );
    assert!(arrangement.accepted().components.is_empty());

    commit(
        &mut arrangement,
        ArrangementOperation::AddSetting {
            reference: APPLICATION_REFERENCE.to_owned(),
            setting_id: "theme.choice".to_owned(),
            container: root(),
            index: usize::MAX,
        },
    );
    let recreated = arrangement.accepted().components.first().unwrap();
    assert_ne!(recreated.key, old.key);
    assert_eq!(recreated.binding, old.binding);
    assert_eq!(recreated.kind.0, SETTING_KIND);
    assert!(recreated.config.is_null());
}

#[test]
fn unavailable_binding_provenance_expires_with_bounded_undo_redo_history() {
    let missing = row("authored-old-row", APPLICATION_REFERENCE, "missing.setting");
    let authored = model(std::slice::from_ref(&missing));
    let config = PaneConfig {
        schema: PaneConfigSchemaId::new(SETTINGS_CONFIG_SCHEMA),
        version: SETTINGS_CONFIG_VERSION,
        payload: serde_json::to_value(EncodedArrangement {
            version: SETTINGS_CONFIG_VERSION,
            composition: authored,
        })
        .unwrap(),
    };
    let live_specs = vec![spec("live.setting")];
    let mut arrangement = SettingsArrangement::restore(
        &config,
        &live_specs,
        sources(APPLICATION_REFERENCE, &live_specs),
    )
    .unwrap();

    let approval =
        DeletionApproval::after_confirmation(missing.key.clone(), true, true, true).unwrap();
    commit(
        &mut arrangement,
        ArrangementOperation::Delete {
            key: missing.key.clone(),
            approval,
        },
    );
    // Keep only one group live at a time so the test exercises provenance
    // eviction rather than hitting the composition's structural limits.
    // Each add/remove pair still advances the accepted history twice.
    for _ in 0..=HISTORY_LIMIT {
        commit(
            &mut arrangement,
            ArrangementOperation::AddGroup {
                key: "history-churn".into(),
                parent: root(),
                index: usize::MAX,
            },
        );
        commit(
            &mut arrangement,
            ArrangementOperation::RemoveEmptyContainer(group("history-churn")),
        );
    }

    let (bindings, descriptors) = arrangement.validation_provenance();
    assert!(!bindings.contains(&(
        missing.key.clone(),
        APPLICATION_REFERENCE.to_owned(),
        "missing.setting".to_owned()
    )));
    assert!(!descriptors.contains(&missing));
    let mut forged = arrangement.accepted().clone();
    forged.components.push(missing.clone());
    forged.placement.push(PlacementNode::Component {
        key: missing.key.clone(),
    });
    assert!(arrangement.validate_candidate(forged).is_err());
}

#[test]
fn default_order_move_group_hide_save_and_restore_preserve_authored_identity() {
    let specs = source_specs();
    let mut arrangement = app_arrangement(&specs);
    let original_keys = arrangement
        .accepted()
        .components
        .iter()
        .map(|row| row.key.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        arrangement.accepted().placement,
        original_keys
            .iter()
            .cloned()
            .map(|key| PlacementNode::Component { key })
            .collect::<Vec<_>>()
    );
    commit(
        &mut arrangement,
        ArrangementOperation::MoveTo {
            key: original_keys[0].clone(),
            container: root(),
            index: 2,
        },
    );
    commit(
        &mut arrangement,
        ArrangementOperation::AddGroup {
            key: "appearance".into(),
            parent: root(),
            index: 0,
        },
    );
    let moved = commit(
        &mut arrangement,
        ArrangementOperation::MoveTo {
            key: original_keys[0].clone(),
            container: group("appearance"),
            index: 0,
        },
    );
    assert!(moved.prepare_routes.contains(&original_keys[0]));
    commit(
        &mut arrangement,
        ArrangementOperation::Hide(original_keys[1].clone()),
    );
    assert!(arrangement.accepted().unplaced.contains(&original_keys[1]));
    let config = arrangement.encode_config().unwrap();
    assert_eq!(config.schema.as_str(), SETTINGS_CONFIG_SCHEMA);
    assert_eq!(config.version, 1);
    assert_eq!(config.payload["version"], json!(1));
    assert!(config.payload.get("collapsed").is_none());
    let restored =
        SettingsArrangement::restore(&config, &specs, sources(APPLICATION_REFERENCE, &specs))
            .unwrap();
    assert_eq!(restored.accepted(), arrangement.accepted());
}

#[test]
fn duplicate_uses_fresh_instance_key_and_delete_undo_reintroduces_clean_descriptor() {
    let specs = source_specs();
    let mut arrangement = app_arrangement(&specs);
    let first = arrangement.accepted().components[0].clone();
    let effect = commit(
        &mut arrangement,
        ArrangementOperation::Duplicate(first.key.clone()),
    );
    let duplicate_key = effect
        .edits
        .iter()
        .find_map(|edit| match edit {
            CompositionEdit::Create(descriptor) => Some(descriptor.key.clone()),
            _ => None,
        })
        .unwrap();
    assert_ne!(duplicate_key, first.key);
    assert!(duplicate_key.0.contains("instance:"));
    let duplicate = arrangement
        .accepted()
        .components
        .iter()
        .find(|item| item.key == duplicate_key)
        .unwrap();
    assert_eq!(duplicate.binding, first.binding);
    let approval =
        DeletionApproval::after_confirmation(duplicate_key.clone(), true, true, true).unwrap();
    let deleted = commit(
        &mut arrangement,
        ArrangementOperation::Delete {
            key: duplicate_key.clone(),
            approval,
        },
    );
    assert!(deleted.removed.contains(&duplicate_key));
    assert!(
        !arrangement
            .accepted()
            .components
            .iter()
            .any(|item| item.key == duplicate_key)
    );
    assert!(matches!(
        DeletionApproval::after_confirmation(first.key.clone(), true, false, true),
        Err(ArrangementError::InvalidOperation(_))
    ));
    let undo = arrangement.preview_undo().unwrap();
    arrangement.commit_undo(undo).unwrap();
    assert!(
        arrangement
            .accepted()
            .components
            .iter()
            .any(|item| item.key == duplicate_key)
    );
    assert!(arrangement.can_redo());
    let redo = arrangement.preview_redo().unwrap();
    arrangement.commit_redo(redo).unwrap();
    assert!(
        !arrangement
            .accepted()
            .components
            .iter()
            .any(|item| item.key == duplicate_key)
    );
}

#[test]
fn invalid_candidates_are_atomic_and_previews_expire_after_another_commit() {
    let specs = source_specs();
    let mut arrangement = app_arrangement(&specs);
    let before = arrangement.accepted().clone();
    let unknown = row("new-row", APPLICATION_REFERENCE, "missing.setting");
    let mut candidate = before.clone();
    candidate.components.push(unknown.clone());
    candidate.placement.push(PlacementNode::Component {
        key: unknown.key.clone(),
    });
    let history = fingerprints(&before);
    let source_map = sources(APPLICATION_REFERENCE, &specs);
    let validator = Validator {
        sources: &source_map,
        historical: &history,
        historical_descriptors: &before.components,
    };
    let mut tx = CompositionTransaction::restore_authored(
        before.clone(),
        CompositionLimits::default(),
        &validator,
    )
    .unwrap();
    assert!(tx.apply(candidate, &validator).is_err());
    assert_eq!(tx.accepted(), &before);
    let stale = arrangement
        .preview(ArrangementOperation::MoveDown(key(
            "turnstone.setting:theme.choice",
        )))
        .unwrap();
    commit(
        &mut arrangement,
        ArrangementOperation::AddGroup {
            key: "g".into(),
            parent: root(),
            index: 0,
        },
    );
    assert!(matches!(
        arrangement.commit(stale),
        Err(ArrangementError::StalePreview)
    ));
}

#[test]
fn unavailable_authored_bindings_and_kinds_restore_but_new_unknowns_are_rejected() {
    let specs = source_specs();
    let initial = row("old-instance-key", APPLICATION_REFERENCE, "theme.choice");
    let authored = Composition {
        components: vec![initial.clone()],
        placement: vec![PlacementNode::Component {
            key: initial.key.clone(),
        }],
        unplaced: Vec::new(),
    };
    let config = PaneConfig {
        schema: PaneConfigSchemaId::new(SETTINGS_CONFIG_SCHEMA),
        version: 1,
        payload: serde_json::to_value(EncodedArrangement {
            version: 1,
            composition: authored,
        })
        .unwrap(),
    };
    let mut restored = SettingsArrangement::restore(&config, &specs, Sources::new()).unwrap();
    assert!(
        commit(
            &mut restored,
            ArrangementOperation::Hide(initial.key.clone())
        )
        .prepare_routes
        .contains(&initial.key)
    );
    assert!(matches!(
        restored.preview(ArrangementOperation::Rebind {
            key: initial.key.clone(),
            reference: APPLICATION_REFERENCE.into(),
            setting_id: "not-registered".into(),
            draft_clean: true,
            no_pending_write: true
        }),
        Err(ArrangementError::InvalidOperation(_))
    ));

    let mut unknown_kind = initial;
    unknown_kind.kind = ComponentKind("missing-old-kind".into());
    unknown_kind.config = json!({"old":"opaque"});
    unknown_kind.binding = json!({"old":"opaque"});
    let old_model = Composition {
        components: vec![unknown_kind.clone()],
        placement: vec![PlacementNode::Component {
            key: unknown_kind.key.clone(),
        }],
        unplaced: Vec::new(),
    };
    let old_config = PaneConfig {
        schema: PaneConfigSchemaId::new(SETTINGS_CONFIG_SCHEMA),
        version: 1,
        payload: serde_json::to_value(EncodedArrangement {
            version: 1,
            composition: old_model,
        })
        .unwrap(),
    };
    let old =
        SettingsArrangement::restore(&old_config, &specs, sources(APPLICATION_REFERENCE, &specs))
            .unwrap();
    assert_eq!(old.accepted().components[0], unknown_kind);
    assert!(matches!(
        old.preview(ArrangementOperation::Duplicate(unknown_kind.key)),
        Err(ArrangementError::InvalidOperation(_))
    ));
}

#[test]
fn rebind_uses_the_requested_registered_reference_and_requires_clean_session() {
    let app_specs = source_specs();
    let other_specs = vec![spec("remote.mode")];
    let mut registry = sources(APPLICATION_REFERENCE, &app_specs);
    registry.extend(sources("fake.second", &other_specs));
    let original = row("custom-instance", APPLICATION_REFERENCE, &app_specs[0].id);
    let mut arrangement =
        SettingsArrangement::from_model(model(&[original.clone()]), registry, false).unwrap();
    assert!(matches!(
        arrangement.preview(ArrangementOperation::Rebind {
            key: original.key.clone(),
            reference: "fake.second".into(),
            setting_id: "remote.mode".into(),
            draft_clean: false,
            no_pending_write: true
        }),
        Err(ArrangementError::InvalidOperation(_))
    ));
    let effect = commit(
        &mut arrangement,
        ArrangementOperation::Rebind {
            key: original.key.clone(),
            reference: "fake.second".into(),
            setting_id: "remote.mode".into(),
            draft_clean: true,
            no_pending_write: true,
        },
    );
    assert!(effect.rebound.contains(&original.key));
    assert_eq!(
        arrangement.accepted().components[0].binding,
        binding("fake.second", "remote.mode")
    );
}

#[test]
fn adding_a_wrapper_before_a_group_marks_nested_routes_for_preparation() {
    let specs = source_specs();
    let descriptors = vec![
        row("custom-a", APPLICATION_REFERENCE, "theme.choice"),
        row("custom-b", APPLICATION_REFERENCE, "shell.zoom"),
    ];
    let composition = Composition {
        components: descriptors.clone(),
        placement: vec![
            PlacementNode::Group {
                key: "g".into(),
                children: vec![PlacementNode::Component {
                    key: descriptors[0].key.clone(),
                }],
            },
            PlacementNode::Component {
                key: descriptors[1].key.clone(),
            },
        ],
        unplaced: Vec::new(),
    };
    let mut arrangement =
        SettingsArrangement::from_model(composition, sources(APPLICATION_REFERENCE, &specs), false)
            .unwrap();
    let effect = commit(
        &mut arrangement,
        ArrangementOperation::AddDrawer {
            key: "new-first".into(),
            parent: root(),
            index: 0,
        },
    );
    assert!(effect.prepare_routes.contains(&descriptors[0].key));
}

#[test]
fn record_validation_checks_config_and_view_state_without_losing_placeholders() {
    let specs = source_specs();
    let mut arrangement = app_arrangement(&specs);
    commit(
        &mut arrangement,
        ArrangementOperation::AddGroup {
            key: "appearance".into(),
            parent: root(),
            index: 0,
        },
    );
    let config = arrangement.encode_config().unwrap();
    let view = encode_view_state(
        arrangement.accepted(),
        &BTreeSet::from([group("appearance")]),
        8.0,
        24.0,
    )
    .unwrap();
    validate_record(&config, &view).unwrap();

    let invalid_view = PaneViewState {
        schema: PaneViewSchemaId::new(SETTINGS_VIEW_SCHEMA),
        version: 1,
        payload: json!({"collapsed":[{"kind":"group","key":"not-authored"}],"scroll":{"x":0.0,"y":0.0}}),
    };
    assert!(validate_record(&config, &invalid_view).is_err());
    let nonfinite = encode_view_state(arrangement.accepted(), &BTreeSet::new(), -1.0, 0.0);
    assert!(nonfinite.is_err());
}

#[test]
fn record_validation_preserves_inert_unknown_sources_and_component_kinds() {
    let specs = source_specs();
    let unavailable = row("remote-row", "fake.second", "remote.mode");
    let unknown_kind = ComponentDescriptor {
        key: key("old-plugin-row"),
        kind: ComponentKind("missing.old-kind".into()),
        config: json!({"opaque": true}),
        binding: json!({"legacy": "payload"}),
    };
    let authored = model(&[unavailable.clone(), unknown_kind.clone()]);
    let config = PaneConfig {
        schema: PaneConfigSchemaId::new(SETTINGS_CONFIG_SCHEMA),
        version: SETTINGS_CONFIG_VERSION,
        payload: serde_json::to_value(EncodedArrangement {
            version: SETTINGS_CONFIG_VERSION,
            composition: authored.clone(),
        })
        .unwrap(),
    };
    let view = PaneViewState {
        schema: PaneViewSchemaId::new(SETTINGS_VIEW_SCHEMA),
        version: SETTINGS_CONFIG_VERSION,
        payload: Value::Null,
    };

    validate_record(&config, &view).unwrap();
    let restored =
        SettingsArrangement::restore(&config, &specs, sources(APPLICATION_REFERENCE, &specs))
            .unwrap();
    assert_eq!(restored.accepted(), &authored);
    let saved_again = restored.encode_config().unwrap();
    validate_record(&saved_again, &view).unwrap();

    // Durable placeholders do not authorize introducing a new target through
    // the live arrangement editor.
    let mut live = SettingsArrangement::from_application_specs(&specs).unwrap();
    assert!(matches!(
        live.preview(ArrangementOperation::AddSetting {
            reference: "fake.second".into(),
            setting_id: "remote.mode".into(),
            container: root(),
            index: 0,
        }),
        Err(ArrangementError::InvalidOperation(_))
    ));
}
