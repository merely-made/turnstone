use mere::forme::FormeRef;

use super::*;

fn graph(n: u128) -> GraphId {
    GraphId::from_uuid(uuid::Uuid::from_u128(n))
}

fn pane(id: u64, kind: &str, source: PaneSource, context: ContextBinding) -> PaneSpec {
    PaneSpec {
        id: PaneId(id),
        kind: PaneKindId::new(kind),
        source,
        context,
        config: PaneConfig::empty(format!("turnstone.{kind}")),
    }
}

fn rect() -> RelativeRect {
    RelativeRect {
        x: 0.1,
        y: 0.1,
        width: 0.5,
        height: 0.5,
    }
}

fn space(id: &str, panes: Vec<PaneSpec>, tiled: LayoutNode) -> SpaceBlueprint {
    SpaceBlueprint {
        id: SpaceId::new(id),
        label: id.into(),
        panes,
        tiled: Some(tiled),
        floating: Vec::new(),
        chrome: ChromeBlueprint::default(),
        normalization: NormalizationPolicy::default(),
    }
}

#[test]
fn context_following_resolves_focus_and_pin_makes_it_fixed() {
    let a = graph(1);
    let b = graph(2);
    let graph_a = pane(
        1,
        "graph",
        PaneSource::Fixed(SourceRef::Forme {
            graph: a,
            forme: FormeRef::Identity(a.0),
        }),
        ContextBinding::Own,
    );
    let graph_b = pane(
        2,
        "graph",
        PaneSource::Fixed(SourceRef::Forme {
            graph: b,
            forme: FormeRef::Identity(b.0),
        }),
        ContextBinding::Own,
    );
    let mut roster = pane(
        3,
        "roster",
        PaneSource::FromContext(SourceSelector::Graph),
        ContextBinding::FocusedInOwnSpace,
    );

    let mut index = ContextIndex::default();
    for id in [graph_a.id, graph_b.id, roster.id] {
        index.place(id, SpaceId::new("primary"));
    }
    index.publish(graph_a.id, graph_a.source.fixed_context().unwrap());
    index.publish(graph_b.id, graph_b.source.fixed_context().unwrap());
    index.focus(graph_a.id);
    index.focus(graph_b.id);
    assert_eq!(index.resolve_source(&roster), Some(SourceRef::Graph(b)));

    roster.pin(SourceRef::Graph(a));
    index.focus(graph_b.id);
    assert_eq!(index.resolve_source(&roster), Some(SourceRef::Graph(a)));
}

#[test]
fn normalization_prunes_unknowns_joins_splits_and_repairs_fractions() {
    let specs = vec![
        pane(
            1,
            "graph",
            PaneSource::Fixed(SourceRef::Graph(graph(1))),
            ContextBinding::Own,
        ),
        pane(
            2,
            "roster",
            PaneSource::Fixed(SourceRef::Graph(graph(1))),
            ContextBinding::Own,
        ),
        pane(
            3,
            "inspector",
            PaneSource::Fixed(SourceRef::Graph(graph(1))),
            ContextBinding::Own,
        ),
    ];
    let nested = LayoutNode::Split {
        axis: SplitAxis::Horizontal,
        children: vec![
            LayoutBranch {
                fraction: 0.2,
                tree: LayoutNode::Pane(PaneId(1)),
            },
            LayoutBranch {
                fraction: 0.8,
                tree: LayoutNode::Split {
                    axis: SplitAxis::Horizontal,
                    children: vec![
                        LayoutBranch {
                            fraction: -4.0,
                            tree: LayoutNode::Pane(PaneId(2)),
                        },
                        LayoutBranch {
                            fraction: 3.0,
                            tree: LayoutNode::Tabs {
                                children: vec![
                                    LayoutNode::Pane(PaneId(999)),
                                    LayoutNode::Pane(PaneId(3)),
                                ],
                                active: 9,
                            },
                        },
                    ],
                },
            },
        ],
    };
    let mut blueprint = space("primary", specs, nested);
    blueprint.normalize();

    let LayoutNode::Split { children, .. } = blueprint.tiled.unwrap() else {
        panic!("same-axis split should normalize to one split");
    };
    assert_eq!(children.len(), 3);
    assert!(children.iter().all(|branch| branch.fraction > 0.0));
    let sum: f32 = children.iter().map(|branch| branch.fraction).sum();
    assert!((sum - 1.0).abs() < 0.0001);
    assert_eq!(children[2].tree, LayoutNode::Pane(PaneId(3)));
}

#[test]
fn floating_relocates_the_same_pane_and_survives_serde() {
    let spec = pane(
        7,
        "settings",
        PaneSource::Fixed(SourceRef::Settings("turnstone/application".into())),
        ContextBinding::Application,
    );
    let mut blueprint = space("primary", vec![spec.clone()], LayoutNode::Pane(spec.id));
    assert!(blueprint.float_pane(spec.id, rect()));
    assert!(blueprint.tiled.is_none());
    assert_eq!(blueprint.floating.len(), 1);
    assert_eq!(blueprint.floating[0].pane, spec.id);
    assert!(validate_spaces(&[blueprint.clone()]).is_empty());

    let json = serde_json::to_string(&blueprint).unwrap();
    let restored: SpaceBlueprint = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, blueprint);
}

#[test]
fn tearout_moves_one_pane_spec_between_spaces_without_changing_identity() {
    let moving = pane(
        11,
        "graph",
        PaneSource::Fixed(SourceRef::Graph(graph(1))),
        ContextBinding::Own,
    );
    let staying = pane(
        12,
        "roster",
        PaneSource::Fixed(SourceRef::Graph(graph(1))),
        ContextBinding::Own,
    );
    let mut primary = space(
        "primary",
        vec![moving.clone(), staying.clone()],
        LayoutNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![
                LayoutBranch {
                    fraction: 0.5,
                    tree: LayoutNode::Pane(moving.id),
                },
                LayoutBranch {
                    fraction: 0.5,
                    tree: LayoutNode::Pane(staying.id),
                },
            ],
        },
    );
    let mut lens = SpaceBlueprint {
        id: SpaceId::new("lens-0"),
        label: "lens".into(),
        panes: Vec::new(),
        tiled: None,
        floating: Vec::new(),
        chrome: ChromeBlueprint::default(),
        normalization: NormalizationPolicy::default(),
    };

    let moved = primary.take_pane(moving.id).unwrap();
    lens.insert_floating(moved, rect()).unwrap();
    assert!(validate_spaces(&[primary, lens]).is_empty());
}

#[test]
fn validation_rejects_one_pane_id_in_two_spaces() {
    let shared = pane(
        19,
        "graph",
        PaneSource::Fixed(SourceRef::Graph(graph(1))),
        ContextBinding::Own,
    );
    let primary = space("primary", vec![shared.clone()], LayoutNode::Pane(shared.id));
    let lens = space("lens", vec![shared.clone()], LayoutNode::Pane(shared.id));

    let violations = validate_spaces(&[primary, lens]);
    assert!(violations.contains(&BlueprintViolation::DuplicatePaneSpec(shared.id)));
    assert!(violations.contains(&BlueprintViolation::DuplicatePaneStation(shared.id)));
}

#[test]
fn validation_rejects_duplicate_stations_and_forme_graph_mismatch() {
    let a = graph(1);
    let b = graph(2);
    let bad = pane(
        20,
        "graph",
        PaneSource::Fixed(SourceRef::Forme {
            graph: a,
            forme: FormeRef::Identity(b.0),
        }),
        ContextBinding::Own,
    );
    let mut blueprint = space("primary", vec![bad.clone()], LayoutNode::Pane(bad.id));
    blueprint.floating.push(FloatingPane {
        pane: bad.id,
        rect: rect(),
        z: 1,
        pinned: false,
        visible: true,
    });
    let violations = validate_spaces(&[blueprint]);
    assert!(violations.contains(&BlueprintViolation::FormeGraphMismatch(bad.id)));
    assert!(violations.contains(&BlueprintViolation::DuplicatePaneStation(bad.id)));
}

#[test]
fn nary_tabs_mixed_surfaces_drag_resize_save_reload_and_tear_out() {
    let graph_id = graph(1);
    let graph_pane = pane(
        1,
        crate::panes::kind::GRAPH,
        PaneSource::Fixed(SourceRef::Forme {
            graph: graph_id,
            forme: FormeRef::Identity(graph_id.0),
        }),
        ContextBinding::Own,
    );
    let document = pane(
        2,
        crate::panes::kind::TILE,
        PaneSource::Fixed(SourceRef::Member {
            graph: graph_id,
            member: uuid::Uuid::from_u128(2),
        }),
        ContextBinding::Own,
    );
    let roster = pane(
        3,
        crate::panes::kind::ROSTER,
        PaneSource::Fixed(SourceRef::Graph(graph_id)),
        ContextBinding::Own,
    );
    let gloss = pane(
        4,
        crate::panes::kind::GLOSS,
        PaneSource::Fixed(SourceRef::Graph(graph_id)),
        ContextBinding::Own,
    );
    let inspector = pane(
        5,
        crate::panes::kind::INSPECTOR,
        PaneSource::Fixed(SourceRef::Member {
            graph: graph_id,
            member: uuid::Uuid::from_u128(2),
        }),
        ContextBinding::Own,
    );
    let mut primary = space(
        "primary",
        vec![
            graph_pane.clone(),
            document.clone(),
            roster.clone(),
            gloss.clone(),
            inspector.clone(),
        ],
        LayoutNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![
                LayoutBranch {
                    fraction: 2.0,
                    tree: LayoutNode::Pane(graph_pane.id),
                },
                LayoutBranch {
                    fraction: 3.0,
                    tree: LayoutNode::Tabs {
                        children: vec![LayoutNode::Pane(document.id), LayoutNode::Pane(roster.id)],
                        active: 0,
                    },
                },
                LayoutBranch {
                    fraction: 5.0,
                    tree: LayoutNode::Split {
                        axis: SplitAxis::Vertical,
                        children: vec![
                            LayoutBranch {
                                fraction: 1.0,
                                tree: LayoutNode::Pane(gloss.id),
                            },
                            LayoutBranch {
                                fraction: 1.0,
                                tree: LayoutNode::Pane(inspector.id),
                            },
                        ],
                    },
                },
            ],
        },
    );

    // The primary presenter, a11y projection, and pump gate see the same
    // active leaves: graph + document + two Cambium panes, never the hidden
    // roster tab.
    assert_eq!(
        primary.active_tiled_panes(),
        vec![PaneId(1), PaneId(2), PaneId(4), PaneId(5)]
    );
    let tiling = place_space(&primary, crate::surface::Rect::full(1200, 800), None);
    assert_eq!(
        tiling.panes.iter().map(|pane| pane.id).collect::<Vec<_>>(),
        primary.active_tiled_panes()
    );
    assert_eq!(
        tiling.dividers.len(),
        3,
        "two N-ary seams plus one nested seam"
    );
    let surfaces = surface_plan_for_space(&primary, crate::surface::Rect::full(1200, 800), None);
    assert!(
        surfaces
            .iter()
            .any(|surface| surface.kind == crate::surface::SurfaceKind::Graph(PaneId(1)))
    );
    assert!(surfaces.iter().any(|surface| {
        surface.kind == crate::surface::SurfaceKind::Content(uuid::Uuid::from_u128(2))
    }));
    assert!(
        surfaces
            .iter()
            .any(|surface| surface.kind == crate::surface::SurfaceKind::Pane(PaneId(4)))
    );
    assert!(
        surfaces
            .iter()
            .all(|surface| surface.kind != crate::surface::SurfaceKind::Pane(PaneId(3)))
    );
    assert_eq!(project_space_blueprint(&primary).nodes.len(), 8);

    // Resize the root, then drag the inspector beside the graph. Its PaneId
    // stays the runtime key across the topology edit.
    assert!(primary.set_split_fractions(&[], &[3.0, 2.0, 5.0]));
    assert!(primary.move_pane_beside(PaneId(5), PaneId(1), SplitAxis::Horizontal, true));
    let runner_state = std::collections::HashMap::from([(PaneId(5), "inspector-scroll")]);
    assert_eq!(runner_state.get(&PaneId(5)), Some(&"inspector-scroll"));

    // A second drag stacks Gloss onto the document tab. Only that active tab
    // remains live until the user selects Roster.
    assert!(primary.stack_pane_onto(PaneId(4), PaneId(2)));
    assert!(primary.active_tiled_panes().contains(&PaneId(4)));
    assert!(!primary.active_tiled_panes().contains(&PaneId(2)));

    // Save/reload preserves the nested N-ary topology and tab selection.
    assert!(primary.activate_tab(PaneId(3)));
    let saved = serde_json::to_string(&primary).expect("blueprint serializes");
    let mut restored: SpaceBlueprint = serde_json::from_str(&saved).expect("blueprint restores");
    assert!(restored.active_tiled_panes().contains(&PaneId(3)));
    assert!(!restored.active_tiled_panes().contains(&PaneId(2)));

    // Tear-out transfers the one spec and its unchanged id into a fresh tiled
    // space, leaving one global pane station and the same retained runner.
    let moved = restored.take_pane(PaneId(5)).expect("inspector exists");
    let mut lens = SpaceBlueprint {
        id: SpaceId::new("lens-0"),
        label: "lens".into(),
        panes: Vec::new(),
        tiled: None,
        floating: Vec::new(),
        chrome: ChromeBlueprint::default(),
        normalization: NormalizationPolicy::default(),
    };
    lens.insert_tiled_root(moved)
        .expect("empty lens accepts pane");
    assert_eq!(lens.active_tiled_panes(), vec![PaneId(5)]);
    assert_eq!(runner_state.get(&PaneId(5)), Some(&"inspector-scroll"));
    assert!(validate_spaces(&[restored, lens]).is_empty());
}
