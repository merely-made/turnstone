// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::door::{bounds, from_wire, to_wire};
use super::fixture::{self, STARTER};
use super::*;
use chirograph::{
    ProjectionSession, RETAINED_PAGES_CEILING, RetainedKeeping, RetainedPagesLimits,
};
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

#[test]
fn limits_apply_before_the_wire_and_every_omission_is_counted() {
    let reply = pollster::block_on(wire_with(RetainedPagesLimits {
        max_pages: 2,
        max_versions: 1,
        max_links: 3,
        ..Default::default()
    }));
    assert_eq!(reply.pages.len(), 2);
    assert_eq!(reply.truncated.pages_omitted, 3);
    assert!(reply.pages.iter().all(|p| p.versions.len() == 1));
    assert!(reply.links.len() <= 3);
    let rebuilt = from_wire("graphshell-mere", &reply).unwrap();
    assert_eq!(rebuilt.skipped_pages, 3, "the viewer keeps the count");
    // A request can never exceed the ceiling.
    let greedy = pollster::block_on(wire_with(RetainedPagesLimits {
        max_pages: u32::MAX,
        max_bytes: u64::MAX,
        max_versions: u32::MAX,
        max_links: u32::MAX,
    }));
    assert_eq!(greedy.applied, RETAINED_PAGES_CEILING);
}

#[test]
fn a_reply_whose_text_does_not_match_its_version_is_refused() {
    let mut reply = pollster::block_on(wire_with(RetainedPagesLimits::default()));
    reply.pages[0].versions[0].text.push_str(" tampered");
    let refusal = from_wire("graphshell-mere", &reply).unwrap_err();
    assert!(refusal.contains("does not match its text"), "{refusal}");
    let mut reply = pollster::block_on(wire_with(RetainedPagesLimits::default()));
    reply.pages[0].current = "not-held".into();
    assert!(from_wire("graphshell-mere", &reply).is_err());
}
