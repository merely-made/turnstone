// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The native accessibility receipt (dynamics grammar plan, F67): the
//! canvas's producer semantics read through cambium-winit-a11y's AccessKit
//! lowering in the headless harness, as a desktop host would hand it to the
//! OS adapter, and a reader's press routed back into the canvas. Then
//! graphshell-client's tree with the viewer's own drag and pin on the local
//! graph's items and the board's cards (F64). Each instrument's positive
//! control is a planted defect it must catch.

use std::cell::RefCell;
use std::rc::Rc;

use accesskit::{NodeId as A11yNodeId, Role, TreeUpdate};
use cambium::{AnyView, GenetCtx, GenetElement, Key, KeyEvent, NamedKey, custom_leaf, el, on_key};
use cambium_genet_winit_host::{
    Harness, HostHooks, Init, ProducedTexture, ProducerContext, TextureProducer, inert_hooks,
};
use cambium_rootstock::{KeyPress, NamedKey as PressKey, ProducedAction, ProducerSemantics};
use cambium_winit_a11y::A11yAction;
use chirograph::{CapabilityProfile, IntentReference, PresentationCapability};
use graphshell_client::LocalActionError;
use mere::canvas::{
    Canvas, DESCRIBED_ITEMS, DRAG_INTENT, PIN_INTENT, PhysicsChoice, Role as ItemRole, Speed,
};

use super::*;
use crate::app::GraphshellApp;
use crate::mere_host::{FIXTURE_PERSONA_ADDRESS, SelectedPersonaRef};
use crate::remote_board::RemoteBoard;

const SLOT: u64 = 3;
const WIDTH: u32 = 1200;
const HEIGHT: u32 = 800;

/// The page: the canvas the slot draws, which its keys steer.
struct Page {
    canvas: Rc<RefCell<Canvas>>,
}

type View = Box<dyn AnyView<Page, (), GenetCtx, GenetElement>>;
type Logic = fn(&Page) -> View;

/// The canvas as a producer, as the tree page's is, minus the pixels.
struct Reader {
    canvas: Rc<RefCell<Canvas>>,
    plant: Plant,
}

impl TextureProducer for Reader {
    fn render(&mut self, _: &ProducerContext<'_>) -> Option<ProducedTexture> {
        None
    }
    fn semantics(&mut self) -> Option<ProducerSemantics> {
        Some(describe_canvas(&self.canvas.borrow(), self.plant))
    }
    fn act(&mut self, key: u64, id: &str) -> bool {
        canvas_act(&mut self.canvas.borrow_mut(), key, id, self.plant)
    }
}

/// The tree page's leaf, minus the pixels: keys go to a keyboard move.
fn view(_: &Page) -> View {
    let leaf = on_key(
        custom_leaf::<Page, ()>(SLOT, WIDTH, HEIGHT).attr(
            "style",
            format!(
                "display:block;position:absolute;left:0;top:0;width:{WIDTH}px;height:{HEIGHT}px;"
            ),
        ),
        |page: &mut Page, event: KeyEvent| {
            let key = match event.key {
                Key::Named(NamedKey::ArrowRight) => MoveKey::Right,
                Key::Named(NamedKey::ArrowDown) => MoveKey::Down,
                Key::Named(NamedKey::Enter) => MoveKey::Drop,
                Key::Named(NamedKey::Escape) => MoveKey::Back,
                _ => return,
            };
            key_move(&mut page.canvas.borrow_mut(), key, 42.0);
        },
    );
    Box::new(el("div", leaf).attr(
        "style",
        format!("position:relative;width:{WIDTH}px;height:{HEIGHT}px;"),
    ))
}

fn sample_canvas() -> Rc<RefCell<Canvas>> {
    let mut canvas = Canvas::with_sample_graph();
    canvas.resize(WIDTH, HEIGHT);
    canvas.set_physics_paused(true);
    canvas.fit_to_content();
    Rc::new(RefCell::new(canvas))
}

fn harness(canvas: Rc<RefCell<Canvas>>, plant: Plant) -> Harness<Page, Logic, View> {
    let mut hooks: HostHooks<Page, Logic, View> = inert_hooks();
    let mut reader = Some(Reader {
        canvas: canvas.clone(),
        plant,
    });
    hooks.frame = Box::new(move |cx| {
        if let Some(reader) = reader.take() {
            cx.producers.register(SLOT, reader, &[]).unwrap();
        }
        false
    });
    let mut host = Harness::with_hooks(
        Init {
            state: Page { canvas },
            logic: view as Logic,
            sheet: String::new(),
            fonts: Vec::new(),
            images: Vec::new(),
        },
        hooks,
    );
    host.prepare_frame();
    host.layout_at(WIDTH as f32, HEIGHT as f32);
    host
}

/// One item as the AccessKit tree tells it: its label, its buttons, and the
/// item key its buttons' actions name (items may share a label).
#[derive(Debug)]
struct Heard {
    label: String,
    buttons: Vec<(A11yNodeId, String, String)>,
    key: Option<u64>,
}

/// The canvas slot's node and its items, read from the projected tree, each
/// item's key read through the produced-action map a press resolves by.
fn hear(
    tree: &TreeUpdate,
    produced: &std::collections::HashMap<A11yNodeId, ProducedAction>,
) -> Option<(String, Vec<Heard>)> {
    let node = |id: A11yNodeId| tree.nodes.iter().find(|(n, _)| *n == id).map(|(_, n)| n);
    let (_, slot) = tree.nodes.iter().find(|(_, n)| {
        n.role() == Role::Group && n.label().is_some_and(|label| label.ends_with(" shown"))
    })?;
    let items = slot
        .children()
        .iter()
        .filter_map(|id| node(*id))
        .map(|item| {
            let buttons: Vec<_> = item
                .children()
                .iter()
                .filter_map(|id| node(*id).map(|button| (*id, button)))
                .filter(|(_, button)| button.role() == Role::Button)
                .map(|(id, button)| {
                    (
                        id,
                        button.label().unwrap_or_default().to_string(),
                        button.description().unwrap_or_default().to_string(),
                    )
                })
                .collect();
            Heard {
                label: item.label().unwrap_or_default().to_string(),
                key: buttons
                    .iter()
                    .find_map(|(id, _, _)| produced.get(id).map(|action| action.key)),
                buttons,
            }
        })
        .collect();
    Some((slot.label().unwrap_or_default().to_string(), items))
}

/// The instrument: the tree a reader is handed names every item pictograph
/// describes, each with its Drag and Pin buttons as advertised, and pressing
/// the first item's Pin pins it. `Err` says which check failed.
fn verify(canvas: &Rc<RefCell<Canvas>>, plant: Plant) -> Result<String, String> {
    let mut h = harness(canvas.clone(), plant);
    let expected = canvas.borrow().describe_items(DESCRIBED_ITEMS);
    let (tree, _) = h.a11y_tree();
    let produced = h.a11y_produced_actions();
    let (slot, items) = hear(&tree, &produced).ok_or("no canvas slot in the tree")?;
    if slot != expected.name() {
        return Err(format!(
            "the slot is named {slot:?}, not {:?}",
            expected.name()
        ));
    }
    for item in &expected.items {
        let heard = items
            .iter()
            .find(|heard| heard.key == Some(item.key) && heard.label == item.name)
            .ok_or_else(|| format!("item {} {:?} missing", item.key, item.name))?;
        for action in &item.actions {
            if !heard.buttons.iter().any(|(_, label, description)| {
                *label == action.label && *description == action.explanation
            }) {
                return Err(format!("{} missing on {:?}", action.label, item.name));
            }
        }
    }
    let first = &expected.items[0];
    let pin = items
        .iter()
        .find(|heard| heard.key == Some(first.key))
        .and_then(|heard| heard.buttons.iter().find(|(_, label, _)| label == "Pin"))
        .ok_or("no Pin to press")?
        .0;
    let action = produced
        .get(&pin)
        .cloned()
        .ok_or("the Pin button names no action")?;
    h.a11y_produced_request(A11yAction::Click, action);
    let role = {
        let canvas = canvas.borrow();
        let key = canvas.graph().get_node_key_by_id(first.member).unwrap();
        canvas.arrangement_role_of(key)
    };
    if role != ItemRole::Pinned {
        return Err(format!("Pin did not route: {:?} is {role:?}", first.name));
    }
    Ok(format!(
        "{slot}: {} items, each with {} buttons",
        items.len(),
        items[0].buttons.len()
    ))
}

/// The receipt, and its positive controls: a missing item, a missing action
/// and an action that does not route each fail the same check.
#[test]
fn a_screen_reader_reaches_the_canvas_items_and_their_buttons() {
    let heard = verify(&sample_canvas(), Plant::None).expect("the real tree passes");
    println!("AccessKit: {heard}");
    for (plant, says) in [
        (Plant::MissingItem, "missing"),
        (Plant::MissingAction, "Pin missing"),
        (Plant::DeadAction, "did not route"),
    ] {
        let failure = verify(&sample_canvas(), plant).expect_err("the plant is caught");
        println!("AccessKit control {plant:?}: {failure}");
        assert!(failure.contains(says), "{plant:?}: {failure}");
    }
}

/// A reader's Drag starts a keyboard move and puts the app's focus on the
/// slot, so the arrows, delivered through the host's key path, nudge the
/// item; Enter drops it, and Escape on a second move puts it back.
#[test]
fn a_readers_drag_starts_a_keyboard_move_the_arrows_steer() {
    let canvas = sample_canvas();
    let mut h = harness(canvas.clone(), Plant::None);
    let item = canvas.borrow().describe_items(DESCRIBED_ITEMS).items[1].clone();
    let (tree, _) = h.a11y_tree();
    let produced = h.a11y_produced_actions();
    let (_, items) = hear(&tree, &produced).unwrap();
    let drag = items
        .iter()
        .find(|heard| heard.key == Some(item.key))
        .and_then(|heard| heard.buttons.iter().find(|(_, label, _)| label == "Drag"))
        .unwrap()
        .0;
    let action: ProducedAction = produced[&drag].clone();
    assert_eq!((action.slot, action.id.as_str()), (SLOT, DRAG_INTENT));
    h.a11y_produced_request(A11yAction::Click, action.clone());
    assert_eq!(canvas.borrow().key_moving(), Some(item.member));
    assert!(h.focus().is_some(), "the slot holds the app's focus");
    let key = canvas
        .borrow()
        .graph()
        .get_node_key_by_id(item.member)
        .unwrap();
    let before = canvas.borrow().screen_position_of(key).unwrap();
    h.press_key(&KeyPress::named(PressKey::ArrowRight));
    h.press_key(&KeyPress::named(PressKey::ArrowDown));
    let after = canvas.borrow().screen_position_of(key).unwrap();
    assert!(
        (after.0 - before.0 - 42.0).abs() < 1e-2 && (after.1 - before.1 - 42.0).abs() < 1e-2,
        "{before:?} to {after:?}"
    );
    h.press_key(&KeyPress::named(PressKey::Enter));
    assert_eq!(canvas.borrow().key_moving(), None, "Enter dropped it");
    assert_eq!(
        canvas.borrow().screen_position_of(key),
        Some(after),
        "a seeded drop stays"
    );

    // A second move, put back by Escape.
    h.a11y_produced_request(A11yAction::Click, action);
    h.press_key(&KeyPress::named(PressKey::ArrowRight));
    h.press_key(&KeyPress::named(PressKey::Escape));
    assert_eq!(canvas.borrow().key_moving(), None);
    let back = canvas.borrow().screen_position_of(key).unwrap();
    assert!(
        (back.0 - after.0).abs() < 1e-2 && (back.1 - after.1).abs() < 1e-2,
        "back"
    );
}

fn app() -> GraphshellApp<muniment::MemoryBackend> {
    GraphshellApp::fixture(
        muniment::MemoryBackend::new(),
        SelectedPersonaRef {
            persona: FIXTURE_PERSONA_ADDRESS.to_string(),
            profile: "profile:graphshell-tree".to_string(),
        },
    )
    .expect("fixture")
}

/// The local route on the local graph's mounted items (F64, F67): each node
/// lists Drag and Pin as local actions, invoking them pins and starts a
/// keyboard move on the canvas, and the endpoint (the mere host) is not
/// touched; an unlisted intent is refused.
#[test]
fn the_clients_tree_lists_the_local_graphs_drag_and_pin_through_the_local_route() {
    let mut app = app();
    let session = app.mount_local().expect("mounted");
    let mut canvas = Canvas::with_graph(app.host.graph().clone());
    canvas.resize(WIDTH, HEIGHT);
    canvas.set_physics_paused(true);
    let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
    let revision = app.host.projection_revision();
    let tree = app
        .client
        .accessibility_tree_with(&session, &profile, &CanvasLocalActions(&mut canvas))
        .expect("tree");
    let listed: Vec<_> = tree
        .children
        .iter()
        .filter(|item| !item.local_actions.is_empty())
        .collect();
    assert_eq!(listed.len(), app.host.graph().node_count(), "every node");
    for item in &listed {
        let intents: Vec<_> = item
            .local_actions
            .iter()
            .map(|a| a.intent.0.as_str())
            .collect();
        assert_eq!(intents, [DRAG_INTENT, PIN_INTENT], "{}", item.label);
    }
    let instance = listed[0].instance;
    let mut local = CanvasLocalActions(&mut canvas);
    assert_eq!(
        app.client.invoke_local(
            &session,
            instance,
            &IntentReference(PIN_INTENT.into()),
            &mut local
        ),
        Ok(true)
    );
    assert_eq!(
        app.client.invoke_local(
            &session,
            instance,
            &IntentReference(DRAG_INTENT.into()),
            &mut local
        ),
        Ok(true)
    );
    assert_eq!(
        app.client.invoke_local(
            &session,
            instance,
            &IntentReference("mere.arrangement.fly".into()),
            &mut local
        ),
        Err(LocalActionError::NotListed)
    );
    let member = canvas.key_moving().expect("a keyboard move");
    let key = canvas.graph().get_node_key_by_id(member).unwrap();
    assert_eq!(canvas.arrangement_role_of(key), ItemRole::Pinned);
    assert_eq!(
        app.host.projection_revision(),
        revision,
        "the endpoint was not asked"
    );
}

/// The local route on the remote board's cards (F64, F67): each card lists
/// the board's Drag and Pin; Pin pins it, after which it lists Pin alone (a
/// pinned card refuses a drag), and Drag starts a keyboard move.
#[test]
fn the_clients_tree_lists_the_boards_drag_and_pin_through_the_local_route() {
    let mut app = app();
    let session = app.mount_fixture_remote().expect("mounted");
    let revision = app.client.mounted(&session).map(|m| m.scene.revision.0);
    let mut board = RemoteBoard::new();
    board.sync(
        app.client.mounted(&session),
        revision,
        Some(&mere::canvas::PhysicsChoice::default().into_spec()),
        Speed::REAL_TIME,
    );
    let profile = CapabilityProfile::new([
        PresentationCapability::NativeGlyph,
        PresentationCapability::PortableCard,
        PresentationCapability::Image,
        PresentationCapability::EditableText,
    ]);
    let intents = |board: &mut RemoteBoard| -> Vec<Vec<String>> {
        app.client
            .accessibility_tree_with(&session, &profile, &BoardLocalActions(board.board_mut()))
            .expect("tree")
            .children
            .iter()
            .map(|item| {
                item.local_actions
                    .iter()
                    .map(|a| a.intent.0.clone())
                    .collect()
            })
            .collect()
    };
    let before = intents(&mut board);
    assert!(before.len() >= 2, "the fixture has cards: {before:?}");
    assert!(before.iter().all(|card| card == &[DRAG_INTENT, PIN_INTENT]));
    let tree = app.client.accessibility_tree(&session, &profile).unwrap();
    let (first, second) = (tree.children[0].instance, tree.children[1].instance);
    let mut local = BoardLocalActions(board.board_mut());
    assert_eq!(
        app.client.invoke_local(
            &session,
            first,
            &IntentReference(PIN_INTENT.into()),
            &mut local
        ),
        Ok(true)
    );
    assert_eq!(
        app.client.invoke_local(
            &session,
            second,
            &IntentReference(DRAG_INTENT.into()),
            &mut local
        ),
        Ok(true)
    );
    assert_eq!(
        board.board().role_of(&first.0.to_string()),
        Some(ItemRole::Pinned)
    );
    assert_eq!(
        board.board().key_moving(),
        Some(second.0.to_string().as_str())
    );
    let after = intents(&mut board);
    assert_eq!(after[0], [PIN_INTENT], "a pinned card lists Pin alone");
}
