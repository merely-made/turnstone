// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn floating_pane_relocates_through_dock_and_window_without_losing_identity() {
    let graph_id = graph(9);
    let graph_pane = pane(
        41,
        crate::panes::kind::GRAPH,
        PaneSource::Fixed(SourceRef::Forme {
            graph: graph_id,
            forme: FormeRef::Identity(graph_id.0),
        }),
        ContextBinding::Own,
    );
    let roster = pane(
        42,
        crate::panes::kind::ROSTER,
        PaneSource::Fixed(SourceRef::Graph(graph_id)),
        ContextBinding::Own,
    );
    let inspector = pane(
        43,
        crate::panes::kind::INSPECTOR,
        PaneSource::Fixed(SourceRef::Member {
            graph: graph_id,
            member: uuid::Uuid::from_u128(43),
        }),
        ContextBinding::Own,
    );
    let mut primary = space(
        "primary",
        vec![graph_pane.clone(), roster.clone(), inspector.clone()],
        LayoutNode::Split {
            axis: SplitAxis::Horizontal,
            children: vec![
                LayoutBranch {
                    fraction: 1.0,
                    tree: LayoutNode::Pane(graph_pane.id),
                },
                LayoutBranch {
                    fraction: 1.0,
                    tree: LayoutNode::Pane(roster.id),
                },
                LayoutBranch {
                    fraction: 1.0,
                    tree: LayoutNode::Pane(inspector.id),
                },
            ],
        },
    );
    let float_rect = RelativeRect {
        x: 0.9,
        y: 0.9,
        width: 0.5,
        height: 0.5,
    };

    // Tile -> float retains the pane key. Proportional geometry is constrained
    // by physical minima/maxima as the window changes size.
    assert!(primary.float_pane(inspector.id, rect()));
    assert!(primary.set_float_rect(inspector.id, float_rect));
    let inspector_constraints = FloatSizeConstraints {
        min_width: 400.0,
        min_height: 200.0,
        max_width: Some(650.0),
        max_height: Some(500.0),
    };
    assert!(primary.set_float_constraints(inspector.id, inspector_constraints));
    assert!(primary.set_float_visible(inspector.id, false));
    assert!(
        place_space(&primary, crate::surface::Rect::full(800, 600), None)
            .floats
            .is_empty()
    );
    assert!(primary.set_float_visible(inspector.id, true));
    assert!(primary.set_float_pinned(inspector.id, true));
    let small = place_space(&primary, crate::surface::Rect::full(800, 600), None);
    assert_eq!(
        small.floats[0].rect,
        crate::surface::Rect::new(400.0, 300.0, 400.0, 300.0)
    );
    let large = place_space(&primary, crate::surface::Rect::full(1600, 1200), None);
    assert_eq!(
        large.floats[0].rect,
        crate::surface::Rect::new(950.0, 700.0, 650.0, 500.0)
    );
    let restored: SpaceBlueprint =
        serde_json::from_str(&serde_json::to_string(&primary).expect("serialize float state"))
            .expect("restore float state");
    assert_eq!(restored.floating[0].constraints, inspector_constraints);
    assert_eq!(restored.floating[0].z, primary.floating[0].z);
    primary = restored;

    // Pinned is a visibility policy rather than a second station: it remains
    // composited and accessible when ordinary floats are hidden.
    let pinned_only = surface_plan_for_space_with_float_layer(
        &primary,
        crate::surface::Rect::full(800, 600),
        None,
        false,
    );
    assert!(
        pinned_only
            .iter()
            .any(|surface| { surface.kind == crate::surface::SurfaceKind::Pane(inspector.id) })
    );
    assert_eq!(
        project_space_blueprint_with_float_layer(&primary, false)
            .nodes
            .len(),
        5
    );

    // A second float proves z-order and focus raise. It is hidden with the
    // layer, while the pinned inspector remains.
    assert!(primary.float_pane(roster.id, rect()));
    assert!(primary.raise_float(roster.id));
    let surfaces = surface_plan_for_space(&primary, crate::surface::Rect::full(800, 600), None);
    let inspector_index = surfaces
        .iter()
        .position(|surface| surface.kind == crate::surface::SurfaceKind::Pane(inspector.id))
        .expect("inspector float surface");
    let roster_index = surfaces
        .iter()
        .position(|surface| surface.kind == crate::surface::SurfaceKind::Pane(roster.id))
        .expect("raised roster float surface");
    assert!(roster_index > inspector_index, "raised float is topmost");
    let hidden_layer = surface_plan_for_space_with_float_layer(
        &primary,
        crate::surface::Rect::full(800, 600),
        None,
        false,
    );
    assert!(
        hidden_layer
            .iter()
            .all(|surface| { surface.kind != crate::surface::SurfaceKind::Pane(roster.id) })
    );

    // Dock both floats. The roster goes into a tab station, then the inspector
    // lands beside the graph inside that tab's existing tree. This proves both
    // dock target forms retain a coherent nested tiled topology.
    assert!(primary.dock_floating_pane(
        roster.id,
        FloatDockTarget::Tab {
            target: graph_pane.id,
        },
    ));
    assert!(primary.dock_floating_pane(
        inspector.id,
        FloatDockTarget::Beside {
            target: graph_pane.id,
            axis: SplitAxis::Horizontal,
            after: true,
        },
    ));
    assert!(primary.floating.is_empty());
    assert_eq!(primary.tiled_panes().len(), 3);

    // Float -> window -> empty tiled root -> float -> return reuses the same
    // pane key throughout. The runner key stands in for retained renderer
    // state and never changes.
    assert!(primary.float_pane(inspector.id, float_rect));
    assert!(primary.set_float_constraints(inspector.id, inspector_constraints));
    assert!(primary.set_float_pinned(inspector.id, true));
    let runner_state = std::collections::HashMap::from([(inspector.id, "scroll=64")]);
    let mut occupied_lens = space(
        "occupied-lens",
        vec![inspector.clone()],
        LayoutNode::Pane(inspector.id),
    );
    let original_float = primary.floating[0].clone();
    assert!(matches!(
        primary.tear_out_floating_pane(inspector.id, &mut occupied_lens),
        Err(BlueprintViolation::DuplicatePaneSpec(id)) if id == inspector.id
    ));
    assert_eq!(primary.floating, vec![original_float]);
    let mut lens = SpaceBlueprint {
        id: SpaceId::new("lens-0"),
        label: "lens".into(),
        panes: Vec::new(),
        tiled: None,
        floating: Vec::new(),
        chrome: ChromeBlueprint::default(),
        normalization: NormalizationPolicy::default(),
    };
    primary
        .tear_out_floating_pane(inspector.id, &mut lens)
        .expect("float tears out to a lens");
    assert_eq!(runner_state.get(&inspector.id), Some(&"scroll=64"));
    assert_eq!(lens.floating[0].pane, inspector.id);
    assert_eq!(lens.floating[0].constraints, inspector_constraints);
    assert!(lens.dock_floating_pane(inspector.id, FloatDockTarget::TiledRoot));
    assert_eq!(lens.active_tiled_panes(), vec![inspector.id]);
    assert!(lens.float_pane(inspector.id, float_rect));
    assert!(lens.set_float_constraints(inspector.id, inspector_constraints));
    assert!(lens.set_float_pinned(inspector.id, true));
    lens.tear_out_floating_pane(inspector.id, &mut primary)
        .expect("float returns to its original space");
    assert!(primary.dock_floating_pane(
        inspector.id,
        FloatDockTarget::Beside {
            target: graph_pane.id,
            axis: SplitAxis::Horizontal,
            after: true,
        },
    ));
    assert_eq!(runner_state.get(&inspector.id), Some(&"scroll=64"));
    assert!(validate_spaces(&[primary, lens]).is_empty());
}

/// A6's fourth clause: the chrome composition restores from a named layout.
///
/// Every other blueprint test builds `ChromeBlueprint::default()`, so the
/// chrome field rode along in serde without any of them being able to tell a
/// restored composition from a fresh default. This pins a composition where
/// all four projections differ from the default and from each other.
#[test]
fn a_named_layout_carries_a_non_default_chrome_composition() {
    let spec = pane(
        3,
        "roster",
        PaneSource::Fixed(SourceRef::Settings("turnstone/application".into())),
        ContextBinding::Application,
    );
    let mut blueprint = space("operate", vec![spec.clone()], LayoutNode::Pane(spec.id));
    // Deliberately four different placements: a default-valued field cannot
    // masquerade as a restored one.
    blueprint.chrome = ChromeBlueprint {
        omnibar: ChromePlacement::Docked(ChromeEdge::Bottom),
        shellbar: ChromePlacement::Hidden,
        transcript: ChromePlacement::Pane(spec.id),
        status: ChromePlacement::Floating,
    };
    assert_ne!(
        blueprint.chrome,
        ChromeBlueprint::default(),
        "the fixture must differ from the default it is proving is not assumed"
    );

    let json = serde_json::to_string(&blueprint).unwrap();
    let restored: SpaceBlueprint = serde_json::from_str(&json).unwrap();

    assert_eq!(
        restored.chrome, blueprint.chrome,
        "every placement restored"
    );
    assert_eq!(
        restored.chrome.transcript,
        ChromePlacement::Pane(spec.id),
        "a transcript projected into a pane keeps that pane's identity"
    );
    assert_eq!(restored, blueprint);
}
