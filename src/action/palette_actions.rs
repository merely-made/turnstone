// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// The palette's action registry: every Action an app-intent lane (the `>`
/// omnibar lane today; automation and a context menu later) may offer, with
/// its display label. The registry is the single catalog those lanes filter;
/// an Action absent here is reachable only by its dedicated input path.
/// Every action the palette offers, label and all.
///
/// The `Layout:` rows are derived from the canvas's
/// `CANVAS_LAYOUT_STRATEGIES` registry
/// rather than written out here, so the plain arrangement names (Spiral,
/// Board, Fractal) have exactly one home and a registry rename reaches the
/// palette without a second edit. Ids stay the technical persistence keys.
pub fn palette_actions() -> Vec<(String, Action)> {
    let mut actions = vec![
        shared(Action::NavBack),
        shared(Action::NavForward),
        shared(Action::Reload),
        shared(Action::Stop),
        (
            "Subscribe to feed: every minute",
            Action::SubscribeFocusedFeed {
                period: servitor::Period::Minute,
            },
        ),
        (
            "Subscribe to feed: hourly",
            Action::SubscribeFocusedFeed {
                period: servitor::Period::Hour,
            },
        ),
        (
            "Subscribe to feed: daily",
            Action::SubscribeFocusedFeed {
                period: servitor::Period::Day,
            },
        ),
        ("Unsubscribe from feed", Action::UnsubscribeFocusedFeed),
        ("Refresh feeds", Action::RefreshFeeds),
        ("Mark feed entry read", Action::MarkFocusedFeedEntryRead),
        ("Reseed layout", Action::ReseedLayout),
        shared(Action::ResetToSeed),
        shared(Action::SetSeed),
        shared(Action::RestoreRecipeSeed),
        shared(Action::ToggleForegroundPin),
        shared(Action::DismissRetained),
        shared(Action::GrowTiles),
        shared(Action::ShrinkTiles),
        shared(Action::ToggleTerritory),
        shared(Action::FitView),
        shared(Action::FoldSelection),
        shared(Action::ExpandFold),
        ("Expand all folds", Action::ExpandAllFolds),
        ("Undo fold", Action::UndoFold),
        ("Redo fold", Action::RedoFold),
        // The per-arrangement `Layout:` rows are derived from the canvas
        // registry below, so the plain display names live in one place.
        // Force-directed is the orrery surface's native arrangement
        // (revert = None) and has no registry row of its own.
        ("Layout: Force-directed", Action::SetLayoutStrategy(None)),
        ("Toggle isometric view", Action::ToggleIsometric),
        ("Toggle height-by-degree", Action::ToggleHeightByDegree),
        ("Toggle size-by-recency", Action::ToggleSizeByRecency),
        shared(Action::TogglePhysics),
        ("Orbit left", Action::OrbitBy(-0.15)),
        ("Orbit right", Action::OrbitBy(0.15)),
        ("Toggle live content", Action::ToggleNodeContent),
        ("Import clip package", Action::ChooseClipPackageFile),
        shared(Action::SaveSession),
        (
            "Open document",
            Action::ChooseKnotDocumentFile { read_only: false },
        ),
        (
            "Open document read-only",
            Action::ChooseKnotDocumentFile { read_only: true },
        ),
        ("Search saved sources", Action::SearchSavedSources),
        ("Ask saved sources", Action::OpenAnswers),
        ("New research note", Action::NewResearchNote),
        ("Open research note", Action::OpenResearchNote),
        #[cfg(feature = "sky")]
        (
            "Open Sky reference",
            Action::SummonContributedPane {
                kind: PaneKindId::new(crate::sky_surface::PANE_KIND),
                source: crate::sky_surface::boston_eclipse_reference_source(),
            },
        ),
    ];
    // A pane the stack names (Settings, Trail, Workbench) takes the
    // catalogue's label, as the verbs above do.
    actions.extend(
        crate::panes::pane_palette_entries()
            .into_iter()
            .map(|(label, kind)| {
                let action = Action::SummonPane(kind);
                (shared_label(&action).unwrap_or(label), action)
            }),
    );
    actions.extend([
        ("Edit themes", Action::OpenThemeWorkshop),
        ("Evidence brief workshop", Action::OpenAlembicWorkshop),
        ("New window", Action::NewWindow),
        ("Float pane", Action::FloatActivePane),
        ("Dock pane", Action::DockActivePane),
        ("Tear out pane", Action::TearOutActivePane),
        ("Return pane to primary", Action::ReturnActivePaneToPrimary),
        ("Fork from node", Action::ForkFocusedNode),
        ("Open node in Workbench", Action::OpenInWorkbench),
        ("Read document", Action::SetWorkspaceMap(false)),
        ("Show document in map", Action::SetWorkspaceMap(true)),
        ("Close workbench tile", Action::CloseWorkbenchTile),
        shared(Action::DeleteFocusedNode),
        ("Empty recycle bin", Action::EmptyRecycleBin),
        ("Close pane", Action::CloseActivePane),
        ("Maximize pane", Action::ToggleMaximizePane),
        ("New session", Action::NewSession),
        ("Rename session", Action::BeginRenameSession),
        ("Close session", Action::CloseSession),
        ("Leave place", Action::LeavePlace),
        ("Reconnect place", Action::ReconnectPlace),
        ("Rejoin place", Action::RejoinPlace),
        ("Place status", Action::ShowPlaceStatus),
    ]);

    let mut rows: Vec<(String, Action)> = actions
        .into_iter()
        .map(|(label, action)| (label.to_string(), action))
        .collect();

    // Insert the registry's arrangements ahead of Force-directed, so every
    // `Layout:` row sits together and the native arrangement closes the group.
    let anchor = rows
        .iter()
        .position(|(_, action)| matches!(action, Action::SetLayoutStrategy(None)))
        .unwrap_or(rows.len());
    for (offset, (id, display_name)) in mere::canvas::CANVAS_LAYOUT_STRATEGIES.iter().enumerate() {
        rows.insert(
            anchor + offset,
            (
                format!("Layout: {display_name}"),
                Action::SetLayoutStrategy(Some(id)),
            ),
        );
    }

    // The physics catalog's rows, derived the same way so the plain names
    // have one home: the laws, then each overlay as an on/off pair, the
    // profiles, and the three attribute sources. They follow the Layout
    // group, after Force-directed. (Physics catalog — P2.)
    let after_layout = rows
        .iter()
        .position(|(_, action)| matches!(action, Action::SetLayoutStrategy(None)))
        .map(|i| i + 1)
        .unwrap_or(rows.len());
    let mut physics: Vec<(String, Action)> = Vec::new();
    for (id, label) in mere::canvas::CANVAS_PHYSICS_LAWS {
        physics.push((format!("Physics: {label}"), Action::SetPhysicsLaw(id)));
    }
    for (id, label) in mere::canvas::CANVAS_PHYSICS_OVERLAYS {
        physics.push((
            format!("Overlay on: {label}"),
            Action::SetPhysicsOverlay(id, true),
        ));
        physics.push((
            format!("Overlay off: {label}"),
            Action::SetPhysicsOverlay(id, false),
        ));
    }
    for profile in mere::canvas::CANVAS_PHYSICS_PROFILES {
        physics.push((
            format!("Profile: {}", profile.label),
            Action::ApplyPhysicsProfile(profile.id),
        ));
    }
    for (id, label) in mere::canvas::CANVAS_PHYSICS_KIND_SOURCES {
        physics.push((format!("Kinds: {label}"), Action::SetPhysicsKindSource(id)));
    }
    for (id, label) in mere::canvas::CANVAS_PHYSICS_MASS_SOURCES {
        physics.push((format!("Mass: {label}"), Action::SetPhysicsMassSource(id)));
    }
    for (id, label) in mere::canvas::CANVAS_PHYSICS_DEPTH_SOURCES {
        physics.push((format!("Depth: {label}"), Action::SetPhysicsDepthSource(id)));
    }
    for role in mere::canvas::Role::ALL {
        physics.push((
            format!("Role: {}", role.label()),
            Action::SetPhysicsRole(role.id()),
        ));
    }
    for (id, label) in mere::canvas::CANVAS_PHYSICS_KIND_SOURCES {
        physics.push((
            format!("Role groups: {label}"),
            Action::SetPhysicsRoleGroups(id),
        ));
    }
    physics.push(("Physics: Use single law".into(), Action::UseSingleLaw));
    for (id, label) in mere::canvas::physics_controls::SPEED_PRESETS {
        physics.push((format!("Speed: {label}"), Action::SetPhysicsSpeed(id)));
    }
    for (offset, row) in physics.into_iter().enumerate() {
        rows.insert(after_layout + offset, row);
    }

    rows
}
