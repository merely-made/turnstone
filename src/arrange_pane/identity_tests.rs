// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
use layout_dom_api::LayoutDom;

fn identity_test_view(state: &ArrangeState) -> ArrangeView {
    choice_row(
        state.choice.status.as_deref().unwrap_or("arrangement"),
        "Arrangement",
        vec![
            ("free".into(), "Free".into()),
            ("grid.default".into(), "Grid".into()),
        ],
        &state.choice.layout,
        ArrangeIntent::SetLayout,
    )
}

fn identity_test_runner() -> (ArrangeRunner, Rc<RefCell<ScriptedDom>>) {
    let app = App::test_stub();
    let dom = Rc::new(RefCell::new(ScriptedDom::new()));
    let mut choice = Choice::of(&app);
    choice.layout = "free".into();
    choice.status = Some("arrangement".into());
    let state = ArrangeState {
        choice,
        form: PhysicsForm::default(),
        form_note: String::new(),
        viewport_w: 0.0,
        viewport_h: 0.0,
    };
    let document = dom.borrow().document();
    let runner = ArrangeRunner::new_at(
        dom.clone(),
        document,
        identity_test_view as fn(&ArrangeState) -> ArrangeView,
        state,
    );
    (runner, dom)
}

fn click_test_radio(runner: &mut ArrangeRunner, dom: &Rc<RefCell<ScriptedDom>>, label: &str) {
    let selector = taproot::Selector::class("radio").containing(label);
    let node = taproot::matching_estimated_fixture(&*dom.borrow(), &selector)[0];
    let _: Vec<ArrangeIntent> = runner.dispatch_click(node, PointerClick::at((0.0, 0.0)));
}

fn apply_test_row(
    runner: &mut ArrangeRunner,
    dom: &Rc<RefCell<ScriptedDom>>,
) -> Vec<ArrangeIntent> {
    let node = taproot::matching_estimated_fixture(
        &*dom.borrow(),
        &taproot::Selector::class("setting-apply"),
    )[0];
    runner.dispatch_click(node, PointerClick::at((0.0, 0.0)))
}

#[test]
fn choice_row_identity_replacement_discards_draft_and_applies_replacement_value() {
    let (mut runner, dom) = identity_test_runner();
    click_test_radio(&mut runner, &dom, "Grid");

    // Same choice control and clean value, but a different setting id.
    // The component's draft belongs to the old binding and must be reset.
    runner.update(|state| state.choice.status = Some("replacement".into()));
    assert_eq!(
        apply_test_row(&mut runner, &dom),
        vec![ArrangeIntent::SetLayout("free".into())]
    );
}

#[test]
fn choice_row_rebuild_for_same_binding_keeps_draft_until_apply() {
    use cambium::{Key, KeyEvent, NamedKey};

    let (mut runner, dom) = identity_test_runner();
    click_test_radio(&mut runner, &dom, "Free");
    let _: Vec<ArrangeIntent> =
        runner.dispatch_key(KeyEvent::new(Key::Named(NamedKey::ArrowRight)));

    // An ordinary parent rebuild with an unchanged binding keeps the
    // user's in-progress selection.
    runner.update(|state| state.choice.regions += 1);
    assert_eq!(
        apply_test_row(&mut runner, &dom),
        vec![ArrangeIntent::SetLayout("grid.default".into())]
    );
}
