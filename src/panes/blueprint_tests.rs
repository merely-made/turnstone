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
        PaneSource::Fixed(SourceRef::Settings("turnstone/appearance".into())),
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
