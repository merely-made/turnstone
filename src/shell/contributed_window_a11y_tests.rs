// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Same-drain delivery through real App transfers, host plans and retained routes.

use super::*;
use crate::action::SpaceRef;
use crate::contributed_surface::tests::{provider, root_text, source};
use crate::contributed_surface::{ContributedSurfaceSessions, SurfaceProviderRegistry};
use crate::panes::{PaneId, PaneKindId, PaneSpec};
use accesskit::{Action as A11yAction, ActionRequest, Role, TreeId};

struct Fixture {
    app: App,
    contents: NodeDocuments,
    sessions: ContributedSurfaceSessions,
    windows: ContributedActionWindows,
    pane: PaneId,
}
fn fixture() -> Fixture {
    let mut app = App::test_stub();
    app.viewport = (800.0, 600.0);
    app.update(Action::SummonContributedPane {
        kind: PaneKindId::new("fake"),
        source: source("fake.v1"),
    });
    let pane = app.active_pane.unwrap();
    app.update(Action::FloatActivePane);
    app.update(Action::TearOutActivePane);
    assert_eq!(app.space_of(pane), Some(SpaceRef::Lens(0)));
    let spec = shared_spec(&app, pane);
    assert_eq!(spec.source, source("fake.v1"));
    let mut registry = SurfaceProviderRegistry::new();
    registry
        .register_provider(provider("fake", "fake.v1", "fake.surface"))
        .unwrap();
    let mut sessions = ContributedSurfaceSessions::default();
    sessions.resolve(&spec, &registry).unwrap();
    let contents = NodeDocuments::default();
    let plan = lens::lens_content_plan(&app, &contents, 0, 800, 600);
    let rect = pane_rect(&plan, pane).unwrap();
    sessions
        .get_mut(pane)
        .unwrap()
        .scene(rect.w as u32, rect.h as u32, 1.0);
    Fixture {
        app,
        contents,
        sessions,
        windows: ContributedActionWindows {
            primary: (800, 600),
            lenses: vec![(0, 800, 600)],
        },
        pane,
    }
}
fn pane_rect(plan: &[crate::surface::Surface], pane: PaneId) -> Option<Rect> {
    plan.iter()
        .find_map(|surface| (surface.kind == SurfaceKind::Pane(pane)).then_some(surface.rect))
}
/// The pane's float record in `space`, and the pixels it resolves to in that
/// window's own 800x600 workspace area.
fn float_in(app: &App, space: SpaceRef, pane: PaneId) -> (workbench::FloatingTile, Rect) {
    let float = app
        .space(space)
        .unwrap()
        .outer_workspace()
        .unwrap()
        .workspace
        .floating()
        .iter()
        .find(|float| float.tile.id.0 == pane.0)
        .expect("pane floats in this space")
        .clone();
    let slot = match space {
        SpaceRef::Primary => 0,
        SpaceRef::Lens(ordinal) => ordinal + 1,
    };
    let area =
        crate::chrome_view::workspace_area_for_window(app.shell_chrome_config(), slot, 800, 600);
    let (x, y, w, h) = float.resolve((area.x, area.y, area.w, area.h));
    (float, Rect::new(x, y, w, h))
}
fn shared_spec(app: &App, pane: PaneId) -> PaneSpec {
    let space = app.space_of(pane).expect("pane retains its actual window");
    assert!(app.blueprint_space(space).is_none(), "shipping transfers keep shared Workspace authority");
    let frame = app.space(space).unwrap().outer_workspace().unwrap();
    let spec: PaneSpec = serde_json::from_value(frame.descriptors.get(&pane.0).expect("shared source descriptor").clone()).unwrap();
    assert_eq!(spec.id,pane);
    assert_eq!(app.pane_content(pane).unwrap().kind_id(),spec.kind);
    spec
}
fn spec(f: &Fixture) -> PaneSpec { shared_spec(&f.app,f.pane) }

fn publish(
    f: &Fixture,
    origin: SpaceRef,
    action: A11yAction,
) -> (a11y_bridge::ActionQueueState, a11y_bridge::QueuedAction) {
    let plan = match origin {
        SpaceRef::Primary => primary_content_plan(&f.app, &f.contents, 800, 600),
        SpaceRef::Lens(ordinal) => lens::lens_content_plan(&f.app, &f.contents, ordinal, 800, 600),
    };
    let pane_rects = plan
        .iter()
        .filter_map(|surface| match surface.kind {
            SurfaceKind::Pane(pane) => Some((pane, surface.rect)),
            _ => None,
        })
        .collect();
    let projected = crate::contributed_a11y::project(&f.sessions, &pane_rects, None);
    let (tree, target) = projected
        .guests
        .iter()
        .find_map(|guest| {
            guest
                .nodes
                .iter()
                .find(|(_, node)| node.role() == Role::Button && node.supports_action(action))
                .map(|(target, _)| (guest.tree_id, *target))
        })
        .unwrap();
    let mut queue = a11y_bridge::ActionQueueState::default();
    queue.publish(
        projected
            .routes
            .into_iter()
            .map(|(key, route)| (key, a11y_bridge::Admission::Contributed(route)))
            .collect(),
    );
    let request = ActionRequest {
        action,
        target_tree: tree,
        target_node: target,
        data: None,
    };
    let captured = queue.published.get(&(tree, target)).unwrap().clone();
    // This captures exactly the synchronous platform queue publication before
    // an earlier host request mutates App. No intermediate publish follows.
    (
        queue,
        a11y_bridge::QueuedAction {
            request,
            admission: Some(captured),
        },
    )
}
fn deliver(
    f: &mut Fixture,
    origin: Option<SpaceRef>,
    queue: &a11y_bridge::ActionQueueState,
    queued: &a11y_bridge::QueuedAction,
) -> bool {
    let captured = queued.admission.as_ref().unwrap();
    assert!(
        queue.is_current(&queued.request, captured),
        "published queue generation alone remains current during drain"
    );
    let a11y_bridge::Admission::Contributed(published) = &captured.admission else {
        panic!("actual contributed admission")
    };
    let plan = contributed_action_plan(&f.app, &f.contents, &f.windows, origin);
    let spec = spec(f);
    dispatch_contributed_action(
        &mut f.app,
        &mut f.sessions,
        published,
        &queued.request,
        &plan,
        Some(&spec),
    )
}
fn return_by_earlier_host_palette_request(f: &mut Fixture) {
    f.app.update(Action::OmnibarOpen { command: true });
    f.app
        .update(Action::OmnibarInsert("Return pane to primary".into()));
    let (tree, routes) = crate::a11y::project_app_with_routes(&f.app);
    let target = tree
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == Role::Button && node.label() == Some("› Return pane to primary")
        })
        .map(|(id, _)| *id)
        .expect("real palette command row is published");
    let request = ActionRequest {
        action: A11yAction::Click,
        target_tree: TreeId::ROOT,
        target_node: target,
        data: None,
    };
    assert!(matches!(
        routes.get(&target),
        Some(crate::a11y::A11yRoute::Action(Action::OmnibarCommitRow(_)))
    ));
    let effects = crate::a11y::apply_route(
        &mut f.app,
        routes.get(&request.target_node),
        request.target_node,
    );
    assert!(!effects.is_empty());
    assert_eq!(f.app.space_of(f.pane), Some(SpaceRef::Primary));
}
fn stale_moved_action(action: A11yAction) {
    let mut f = fixture();
    let (queue, queued) = publish(&f, SpaceRef::Lens(0), action);
    let captured = queued.admission.as_ref().unwrap();
    let a11y_bridge::Admission::Contributed(old) = &captured.admission else {
        unreachable!()
    };
    let old = old.clone();
    let before_generation = f.sessions.get(f.pane).unwrap().generation();
    assert_eq!(f.sessions.get(f.pane).unwrap().accessibility_focus(), None);
    let (lens_float, lens_window) = float_in(&f.app, SpaceRef::Lens(0), f.pane);
    return_by_earlier_host_palette_request(&mut f);
    let current_primary = primary_content_plan(&f.app, &f.contents, 800, 600);
    let current_lens = lens::lens_content_plan(&f.app, &f.contents, 0, 800, 600);
    // Floats are proportional to each window's workspace, and only the
    // primary window reserves a navigation row, so the transfer keeps the
    // relative record and the body's inset rather than lens pixels.
    let (primary_float, primary_window) = float_in(&f.app, SpaceRef::Primary, f.pane);
    assert_eq!(
        (primary_float.rect, primary_float.constraints),
        (lens_float.rect, lens_float.constraints),
        "floating transfer preserves exact window geometry"
    );
    let inset = |body: Rect, window: Rect| {
        [
            body.x - window.x,
            body.y - window.y,
            window.w - body.w,
            window.h - body.h,
        ]
    };
    let body = pane_rect(&current_primary, f.pane).unwrap();
    for (now, before) in inset(body, primary_window)
        .into_iter()
        .zip(inset(old.rect, lens_window))
    {
        assert!(
            // Body rects are snapped to whole pixels after layout.
            (now - before).abs() < 1.0,
            "the body keeps its inset inside the transferred float: {body:?} in {primary_window:?}, was {:?} in {lens_window:?}",
            old.rect
        );
    }
    assert_eq!(pane_rect(&current_lens, f.pane), None);
    assert_eq!(
        f.sessions.get(f.pane).unwrap().generation(),
        before_generation
    );
    assert_eq!(
        f.sessions
            .get(f.pane)
            .unwrap()
            .accessibility_tree()
            .unwrap()
            .0,
        *old.source,
        "same retained source must not hide the window move"
    );
    assert!(
        !deliver(&mut f, Some(SpaceRef::Lens(0)), &queue, &queued),
        "old lens request must be refused in the same ordered drain"
    );
    let session = f.sessions.get(f.pane).unwrap();
    assert_eq!(session.accessibility_focus(), None);
    assert!(root_text(&session.session().dom(), session.session().root()).contains("count:0"));
    let (fresh, request) = publish(&f, SpaceRef::Primary, action);
    assert!(
        deliver(&mut f, Some(SpaceRef::Primary), &fresh, &request),
        "fresh new-window publication remains usable"
    );
    let session = f.sessions.get(f.pane).unwrap();
    match action {
        A11yAction::Focus => {
            assert_eq!(session.accessibility_focus(), Some(old.route.node));
            assert_eq!(f.app.focus, crate::surface::FocusTarget::Pane(f.pane));
        },
        A11yAction::Click => assert!(
            root_text(&session.session().dom(), session.session().root()).contains("count:1")
        ),
        _ => unreachable!(),
    }
}
#[test]
fn contributed_window_action_same_drain_moved_focus_rejects_old_lens() {
    stale_moved_action(A11yAction::Focus);
}
#[test]
fn contributed_window_action_same_drain_moved_click_rejects_old_lens() {
    stale_moved_action(A11yAction::Click);
}
#[test]
fn contributed_window_action_unchanged_lens_focus_and_click_controls() {
    for action in [A11yAction::Focus, A11yAction::Click] {
        let mut f = fixture();
        let (queue, queued) = publish(&f, SpaceRef::Lens(0), action);
        assert!(deliver(&mut f, Some(SpaceRef::Lens(0)), &queue, &queued));
        let session = f.sessions.get(f.pane).unwrap();
        match action {
            A11yAction::Focus => assert!(session.accessibility_focus().is_some()),
            A11yAction::Click => assert!(
                root_text(&session.session().dom(), session.session().root()).contains("count:1")
            ),
            _ => unreachable!(),
        }
    }
}
