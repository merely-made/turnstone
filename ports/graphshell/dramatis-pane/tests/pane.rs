// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The pane as a host renders it: retained views, layout, focus and intent
//! routing, with the same expectations the Dramatis window's tests held.
use std::cell::RefCell;
use std::rc::Rc;

use cambium::{el, text};
use cambium_genet_winit_host::{Harness, Init, Key, KeyPress, NamedKey};
use castellan_client::quarantine::{QuarantineCard, ReviewAction, UnassignedImport};
use dramatis::{
    roster::{Roster, RosterEntry},
    view::VaultLockView,
};
use dramatis_surface::{Custody, Reply, Request, Snapshot};
use graphshell_dramatis_pane::{Child as PaneChild, Pane, SHEET, follow_focus_hooks, view};
use personae::ProfileId;
use taproot::Selector;

#[derive(Clone, Default)]
struct Fake {
    sent: Rc<RefCell<Vec<Request>>>,
    replies: Rc<RefCell<Vec<(Reply, bool)>>>,
}
impl Custody for Fake {
    fn send(&mut self, request: Request) {
        self.sent.borrow_mut().push(request);
    }
    fn drain(&mut self) -> Vec<(Reply, bool)> {
        std::mem::take(&mut *self.replies.borrow_mut())
    }
}

struct App {
    pane: Pane<Fake>,
}
type Child = Box<dyn cambium::AnyView<App, (), cambium::GenetCtx, cambium::GenetElement>>;
type Logic = fn(&App) -> Child;

fn root(app: &App) -> Child {
    let pane: PaneChild<App> = view(&app.pane, |app: &mut App| &mut app.pane);
    Box::new(el("main", pane).attr("class", "dramatis"))
}

const PAGE: &str = "main { background: #f4f1e9; color: #25251f; font-family: system-ui; height: 100vh; box-sizing: border-box; overflow-y: auto; overflow-x: hidden; }";

fn item() -> chatelaine::Item {
    chatelaine::Item {
        id: chatelaine::ItemId::from_random([1; 16]),
        source_id: None,
        title: "Imported payment card for review".into(),
        subtitle: None,
        scope: None,
        tags: vec![],
        favorite: false,
        created_at: None,
        modified_at: None,
        credentials: vec![chatelaine::Credential {
            id: chatelaine::CredentialId::from_random([2; 16]),
            kind: chatelaine::CredentialKind::CreditCard {
                card_type: None,
                expiry: None,
                last_four: None,
            },
        }],
        state: chatelaine::ItemState::Quarantined,
    }
}

fn snapshot(locked: bool, unsupported: bool, older: bool) -> Snapshot {
    let entry = |id: &str, name: &str, chosen| RosterEntry {
        id: ProfileId(id.into()),
        display_name: name.into(),
        slot_count: 0,
        chosen,
    };
    Snapshot {
        lock: if locked {
            VaultLockView::Locked
        } else {
            VaultLockView::Unlocked
        },
        roster: Roster {
            chosen: ProfileId("family".into()),
            description: String::new(),
            entries: vec![
                entry("family", "Family", true),
                entry("work", "Work", false),
            ],
        },
        unassigned: if older && !locked {
            vec![UnassignedImport {
                store: ProfileId("family".into()),
                persona: *personae::PersonaId::new().as_uuid(),
                titles: vec!["Older imported card".into()],
                item_count: 1,
                revision: "older-reviewed-metadata".into(),
            }]
        } else {
            vec![]
        },
        cards: if locked {
            vec![]
        } else {
            vec![QuarantineCard {
                store: ProfileId("family".into()),
                persona: *personae::PersonaId::new().as_uuid(),
                item: item(),
                revision: "exact-reviewed-metadata".into(),
                accept_blocked: unsupported
                    .then(|| "This credential type is not supported.".into()),
            }]
        },
    }
}

struct Rig {
    h: Harness<App, Logic, Child>,
    fake: Fake,
}
fn rig(snapshot: Snapshot, width: f32, height: f32) -> Rig {
    let fake = Fake::default();
    fake.replies.borrow_mut().push((
        Reply {
            snapshot: Some(snapshot),
            notice: String::new(),
        },
        true,
    ));
    let pane = Pane::new(fake.clone());
    let mut h = Harness::with_hooks(
        Init {
            state: App { pane },
            logic: root as Logic,
            sheet: format!("{PAGE}{SHEET}"),
            fonts: vec![],
            images: vec![],
        },
        follow_focus_hooks(),
    );
    h.layout_at(width, height);
    Rig { h, fake }
}
impl Rig {
    fn sent(&self) -> Vec<Request> {
        self.fake.sent.borrow().clone()
    }
    /// What the host does on the worker's wake: take the reply and redraw.
    fn answer(&mut self, snapshot: Option<Snapshot>, notice: &str) {
        self.fake.replies.borrow_mut().push((
            Reply {
                snapshot,
                notice: notice.into(),
            },
            true,
        ));
        self.h.update(|app| app.pane.refresh());
    }
    fn button(&self, label: &str) -> Selector {
        Selector::role("button").containing(label)
    }
    fn buttons(&self, label: &str) -> usize {
        self.h.with_dom(|dom| {
            taproot::matching_estimated_fixture(dom, &Selector::role("button").containing(label))
                .len()
        })
    }
}
fn enter() -> KeyPress {
    KeyPress {
        key: Key::Named(NamedKey::Enter),
        text: None,
        modifiers: Default::default(),
        repeat: false,
    }
}

#[test]
fn keyboard_reconnect_and_pointer_persona_selection_use_the_real_controls() {
    let mut r = rig(snapshot(false, false, false), 860.0, 720.0);
    r.h.tab(true);
    assert!(r.h.focus().is_some());
    r.h.press_key(&enter());
    assert!(matches!(r.sent().as_slice(), [Request::Refresh]));
    r.answer(Some(snapshot(false, false, false)), "");
    assert!(r.h.click_on(&r.button("Choose Work")));
    assert!(matches!(r.sent().last(), Some(Request::Select(ProfileId(p))) if p == "work"));
}

#[test]
fn unsupported_acceptance_is_not_a_control_but_delete_names_the_exact_card() {
    let mut r = rig(snapshot(false, true, false), 420.0, 720.0);
    assert_eq!(r.buttons("Accept"), 0);
    assert!(!r.h.click_on(&r.button("Accept")));
    assert!(r.sent().is_empty());
    assert!(r.h.click_on(&r.button("Delete")));
    assert!(matches!(r.sent().as_slice(),
        [Request::Review(q)] if q.action == ReviewAction::Delete && q.profile.0 == "family" && q.revision == "exact-reviewed-metadata"));
}

#[test]
fn locked_persona_buttons_do_not_exist_and_unlock_queues_a_request() {
    let mut r = rig(snapshot(true, false, false), 420.0, 720.0);
    assert_eq!(r.buttons("Choose"), 0);
    assert!(!r.h.click_on(&r.button("Choose Work")));
    assert!(r.sent().is_empty());
    assert!(r.h.click_on(&r.button("Unlock with Djinn")));
    assert!(matches!(r.sent().as_slice(), [Request::Unlock]));
}

#[test]
fn narrow_layout_and_zoom_keep_a_review_action_reachable() {
    let mut r = rig(snapshot(false, false, false), 420.0, 720.0);
    r.h.set_ui_zoom(2.0);
    for _ in 0..12 {
        r.h.tab(true);
        let focus = r.h.focus().expect("keyboard has a focused control");
        assert!(
            r.h.visible_rect(focus).is_some(),
            "every tabbed control is revealed at double zoom"
        );
    }
    let node = r.h.focus().unwrap();
    let (_, _, w, height) =
        r.h.visible_rect(node)
            .expect("focused control is revealed in viewport");
    assert!(w > 0.0 && height > 0.0);
    r.h.set_ui_zoom(1.0);
    r.h.layout_at(860.0, 720.0);
    assert!(r.h.click_on(&r.button("Accept")));
    assert!(
        matches!(r.sent().as_slice(), [Request::Review(q)] if q.action == ReviewAction::Accept)
    );
}

#[test]
fn older_import_assignment_names_the_destination_and_exact_revision() {
    let mut r = rig(snapshot(false, false, true), 860.0, 1400.0);
    assert!(r.h.click_on(&r.button("Assign to this persona")));
    // The form opened and nothing is sent until a destination is chosen.
    assert!(r.sent().is_empty());
    let submit = Selector::role("button").with_attr("data-dramatis-submit", "true");
    assert!(r.h.click_on(&submit));
    assert!(r.sent().is_empty(), "no destination, no request");
    assert!(r.h.click_on(&r.button("Family")));
    assert!(r.h.click_on(&submit));
    assert!(matches!(r.sent().as_slice(),
        [Request::Assign(a)] if a.profile.0 == "family" && a.revision == "older-reviewed-metadata"));
}

#[test]
fn focus_follow_preserves_manual_scrolling_until_focus_changes() {
    let mut r = rig(snapshot(false, false, false), 420.0, 720.0);
    r.h.set_ui_zoom(2.0);
    let delete = r.h.with_dom(|dom| {
        taproot::matching_estimated_fixture(dom, &Selector::role("button").containing("Delete"))[0]
    });
    for _ in 0..16 {
        if r.h.focus() == Some(delete) {
            break;
        }
        r.h.tab(true);
    }
    assert_eq!(r.h.focus(), Some(delete));
    assert!(r.h.visible_rect(delete).is_some());
    let before = r.h.element_scroll_total();
    assert!(before > 0.0);
    r.h.move_to(100.0, 100.0);
    r.h.wheel(0.0, -2000.0);
    assert_eq!(r.h.focus(), Some(delete));
    assert!(
        r.h.element_scroll_total() < before,
        "manual scroll is not undone by focus following"
    );
    r.h.tab(true);
    assert!(r.h.visible_rect(r.h.focus().unwrap()).is_some());
}

#[test]
fn locked_vault_keyboard_order_holds_only_the_controls_the_surface_offers() {
    let mut r = rig(snapshot(true, false, false), 420.0, 720.0);
    let persona_buttons = r.buttons("Family") + r.buttons("Work");
    assert_eq!(persona_buttons, 0);
    let allowed = r.h.with_dom(|dom| {
        let mut nodes = taproot::matching_estimated_fixture(
            dom,
            &Selector::role("button").containing("Reconnect"),
        );
        nodes.extend(taproot::matching_estimated_fixture(
            dom,
            &Selector::role("button").containing("Unlock"),
        ));
        nodes
    });
    assert_eq!(allowed.len(), 2);
    for _ in 0..8 {
        r.h.tab(true);
        assert!(
            allowed.contains(&r.h.focus().unwrap()),
            "only offered controls are keyboard stops"
        );
    }
}

#[test]
fn authentication_control_is_a_public_request_and_absent_when_locked() {
    for locked in [false, true] {
        let mut r = rig(snapshot(locked, false, false), 860.0, 720.0);
        let activated = r.h.click_on(&r.button("Change passphrase"));
        if locked {
            assert!(!activated);
            assert!(r.sent().is_empty());
        } else {
            assert!(activated);
            assert!(matches!(r.sent().as_slice(), [Request::ChangePassphrase]));
        }
    }
}

#[test]
fn authentication_can_retry_after_a_presented_busy_state_and_cancellation() {
    let mut r = rig(snapshot(false, false, false), 860.0, 720.0);
    let change = r.button("Change passphrase");
    assert!(r.h.click_on(&change));
    assert!(r.h.state().pane.busy());
    // While Djinn is at its prompt the control stays but is disabled and out
    // of the keyboard order: a second press queues nothing.
    r.h.layout_at(860.0, 720.0);
    r.h.click_on(&change);
    assert_eq!(r.sent().len(), 1);
    r.answer(
        Some(snapshot(false, false, false)),
        "Passphrase change cancelled. Your passphrase is unchanged.",
    );
    assert!(!r.h.state().pane.busy());
    assert!(r.h.click_on(&change));
    assert_eq!(r.sent().len(), 2);
    assert!(matches!(r.sent().last(), Some(Request::ChangePassphrase)));
}

#[test]
fn busy_controls_are_disabled_and_are_not_keyboard_stops() {
    let mut r = rig(snapshot(false, false, false), 420.0, 720.0);
    assert!(r.h.click_on(&r.button("Reconnect")));
    r.h.layout_at(420.0, 720.0);
    let controls = r.h.with_dom(|dom| {
        taproot::matching_estimated_fixture(dom, &Selector::role("button").containing("Delete"))
    });
    assert_eq!(controls.len(), 1, "the control keeps its place");
    for _ in 0..8 {
        r.h.tab(true);
        assert!(r.h.focus().is_none_or(|f| !controls.contains(&f)));
    }
}

#[test]
fn feedback_keeps_its_live_region_and_heading_stable() {
    let mut r = rig(snapshot(false, false, false), 860.0, 720.0);
    let node = |r: &Rig, selector: Selector| {
        r.h.with_dom(|dom| {
            let nodes = taproot::matching_classes(dom, &selector);
            assert_eq!(nodes.len(), 1, "{selector:?}");
            nodes[0]
        })
    };
    let status_selector = Selector::class("dramatis-status").with_attr("role", "status");
    let heading_selector = Selector::class("dramatis-heading").containing("Dramatis");
    let status = node(&r, status_selector.clone());
    let heading = node(&r, heading_selector.clone());
    let before = r.h.painted_rect(heading).unwrap();
    let before_font = r.h.computed_value(heading, "font-size");
    for notice in ["Passphrase changed.", ""] {
        r.answer(Some(snapshot(false, false, false)), notice);
        assert_eq!(node(&r, status_selector.clone()), status);
        assert_eq!(node(&r, heading_selector.clone()), heading);
        assert_eq!(r.h.computed_value(heading, "font-size"), before_font);
        let after = r.h.painted_rect(heading).unwrap();
        assert!(
            (after.3 - before.3).abs() < 0.1,
            "feedback must not turn the heading into a control"
        );
    }
}

#[test]
fn an_absent_djinn_leaves_a_status_region_and_reconnect() {
    let mut r = rig(snapshot(false, false, false), 860.0, 720.0);
    r.answer(
        None,
        "Djinn is unavailable. Identity is pending. Start Djinn, then reconnect.",
    );
    assert!(r.buttons("Reconnect") == 1);
    assert_eq!(r.buttons("Choose"), 0);
    assert!(r.h.click_on(&r.button("Reconnect")));
    assert!(matches!(r.sent().as_slice(), [Request::Refresh]));
    let _ = text("");
}
