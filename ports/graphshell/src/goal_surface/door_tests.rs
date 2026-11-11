// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use graphshell_host::goal_surface::door::{bounds, from_wire, to_wire};
use graphshell_host::goal_surface::fixture::{self, STARTER};
use super::*;
use graphshell_host::goal_surface::{GraphFacts, RetainedPages};
use chirograph::{ProjectionSession, RetainedKeeping, RetainedPagesLimits};
use std::rc::Rc;

async fn wire_with(limits: RetainedPagesLimits) -> chirograph::RetainedPagesResponse {
    let (backend, graph) = fixture::backend_and_graph().await;
    let limits = limits.effective();
    let pages = RetainedPages::load_bounded(
        "resident",
        &backend,
        &GraphFacts::read(&graph),
        bounds(limits),
        false,
    )
    .await
    .unwrap();
    to_wire(&pages, ProjectionSession("mere:graph".into()), limits, RetainedKeeping::On)
}

#[test]
fn the_goal_surface_runs_unchanged_over_pages_rebuilt_from_the_door_reply() {
    let reply = pollster::block_on(wire_with(RetainedPagesLimits::default()));
    assert!(reply.truncated.is_complete());
    assert_eq!(reply.pages.len(), 5);
    // The unretained hotel page is named, with the link that points at it.
    assert_eq!(reply.unretained.len(), 1);
    assert!(reply.links.len() >= 7);
    let rebuilt = from_wire("graphshell-mere", &reply).unwrap();
    let mut surface = GoalSurface::new(Rc::new(rebuilt));
    surface.goal.insert_str("keep a starter alive while travelling");
    surface.run();
    let run = surface.run.as_ref().expect("a run");
    assert!(run.matched >= 1, "{:?}", surface.notice);
    // The citation names the exact version read, with its text.
    let cited = run
        .steps
        .iter()
        .find(|step| step.address == STARTER)
        .expect("the starter page is read");
    assert!(matches!(cited.outcome, Outcome::Visited(_)));
    // Both versions of the earlier-captured page travelled.
    let page = chartulary::resource_id(STARTER).to_string();
    let wire_page = reply.pages.iter().find(|p| p.id == page).unwrap();
    assert_eq!(wire_page.versions.len(), 2);
    assert_eq!(wire_page.versions[0].version, wire_page.current);
}

