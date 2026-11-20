// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use chirograph::RetainedPagesLimits;
use graphshell_host::goal_surface::fixture::{self, INDEX, STARTER};

const GOAL: &str = "keep a starter alive while travelling";

fn wire(pages: &RetainedPages, keeping: RetainedKeeping) -> RetainedPagesResponse {
    door::to_wire(
        pages,
        ProjectionSession("mere.graph".into()),
        RetainedPagesLimits::default().effective(),
        keeping,
    )
}

fn apply_wire(panel: &mut Panel, wire: RetainedPagesResponse) {
    panel.apply(&Reply {
        pages: KeptReading::from_wire(wire),
        notice: None,
    });
}

fn choose_start(panel: &mut Panel) -> &mut GoalSurface {
    let surface = panel.traversal.as_mut().expect("admitted goal snapshot");
    let index = surface
        .pages()
        .sources()
        .iter()
        .position(|(_, page)| page.url == INDEX)
        .expect("index page");
    surface.start = Start::Source(index);
    surface.goal = TextInput::new(GOAL);
    surface
}

#[tokio::test]
async fn kept_search_and_goal_follow_the_same_admitted_pages() {
    let pages = fixture::pages().await;
    let mut panel = Panel {
        submitted: "refrigerate".into(),
        ..Default::default()
    };
    apply_wire(&mut panel, wire(&pages, RetainedKeeping::On));
    let hit = panel.hits.first().expect("kept text search found the page");
    let page = panel.page(&hit.key).expect("the search hit is readable");
    assert_eq!(page.url, STARTER);
    panel.selected = Some((hit.key.clone(), page.current.clone()));
    assert!(panel.reading().unwrap().1.contains("refrigerate"));

    let surface = choose_start(&mut panel);
    surface.max_depth = TextInput::new("0");
    surface.run();
    let at_start = surface.run.as_ref().unwrap();
    assert_eq!(at_start.steps[0].address, INDEX);
    assert!(!at_start.matches().any(|step| step.address == STARTER));

    surface.max_depth = TextInput::new("1");
    surface.run();
    assert!(
        surface
            .run
            .as_ref()
            .unwrap()
            .matches()
            .any(|step| step.address == STARTER),
        "the cited page is reachable only through its retained link"
    );
    assert!(surface.receipt.is_some());
}

#[tokio::test]
async fn paused_unavailable_and_changed_scope_clear_kept_material() {
    let pages = fixture::pages().await;
    for change in ["paused", "unavailable", "scope"] {
        let mut panel = Panel {
            open: true,
            submitted: "refrigerate".into(),
            follow_links: true,
            ..Default::default()
        };
        apply_wire(&mut panel, wire(&pages, RetainedKeeping::On));
        let hit = panel.hits.first().expect("kept text search found the page");
        let page = panel.page(&hit.key).expect("the search hit is readable");
        panel.selected = Some((hit.key.clone(), page.current.clone()));
        let surface = choose_start(&mut panel);
        surface.run();
        assert!(surface.receipt.is_some(), "{change}");

        match change {
            "paused" => {
                apply_wire(
                    &mut panel,
                    wire(
                        &pages,
                        RetainedKeeping::Paused {
                            reason: "custody locked".into(),
                        },
                    ),
                );
                assert!(panel.status().contains("custody locked"));
            },
            "unavailable" => panel.apply(&Reply {
                pages: Err("The resident stopped serving this route.".into()),
                notice: None,
            }),
            "scope" => panel.reset_scope(),
            _ => unreachable!(),
        }

        assert!(panel.open, "{change}");
        assert!(panel.traversal.is_none(), "{change}");
        assert!(panel.hits.is_empty(), "{change}");
        assert!(panel.selected.is_none(), "{change}");
    }
}
