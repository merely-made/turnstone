// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Shared controls over the existing Tabard browser preference session.
use super::*;
use cambium::{SelectState, TextInput, button, lens, select, textarea_typed};

pub(super) struct Controls {
    open: bool,
    theme: SelectState,
    mode: SelectState,
    themes: Vec<(String, String)>,
    modes: Vec<(String, String)>,
    previous: (usize, usize),
    input: TextInput,
}
impl Controls {
    pub(super) fn new(session: &crate::web_appearance::Session) -> Self {
        let mut value = Self {
            open: false,
            theme: SelectState::new(0).with_label("Appearance theme"),
            mode: SelectState::new(0).with_label("Appearance mode"),
            themes: Vec::new(),
            modes: Vec::new(),
            previous: (0, 0),
            input: TextInput::new(""),
        };
        value.sync(session);
        value
    }
    fn sync(&mut self, session: &crate::web_appearance::Session) {
        if let Some(model) = session.controls() {
            self.themes = model.themes;
            self.modes = model.modes;
            self.theme.selected = self
                .themes
                .iter()
                .position(|(id, _)| id == &model.theme)
                .unwrap_or(0);
            self.mode.selected = self
                .modes
                .iter()
                .position(|(id, _)| id == &model.mode)
                .unwrap_or(0);
            self.previous = (self.theme.selected, self.mode.selected);
        }
    }
    pub(super) fn changed(&self) -> bool {
        self.previous != (self.theme.selected, self.mode.selected)
    }
}
pub(super) fn apply(page: &mut TreePage) {
    let control = &mut page.appearance_controls;
    let Some((theme, _)) = control.themes.get(control.theme.selected) else {
        return;
    };
    let Some((mode, _)) = control.modes.get(control.mode.selected) else {
        return;
    };
    let mut session = page.shared.appearance.borrow_mut();
    if let Err(error) = session.choose(theme, mode) {
        web_sys::console::error_1(&error.into());
    }
    control.sync(&session);
}
pub(super) fn view(page: &TreePage) -> Option<Child> {
    let model = page.shared.appearance.borrow().controls()?;
    let control = &page.appearance_controls;
    let mut children: Vec<Child> = vec![Box::new(
        button("Appearance", |page: &mut TreePage, _| {
            page.appearance_controls.open = !page.appearance_controls.open;
        })
        .attr("aria-expanded", control.open.to_string()),
    )];
    // Status remains visible when the disclosure is closed, including a reopened preference.
    children.push(Box::new(
        el("p", model.status)
            .attr("role", "status")
            .attr("aria-label", "Appearance status"),
    ));
    if control.open {
        let themes: Vec<String> = control
            .themes
            .iter()
            .map(|(_, label)| label.clone())
            .collect();
        let modes: Vec<String> = control
            .modes
            .iter()
            .map(|(_, label)| label.clone())
            .collect();
        children.push(Box::new(lens(
            move |state: &mut SelectState| {
                select(
                    state,
                    &themes.iter().map(String::as_str).collect::<Vec<_>>(),
                )
            },
            |page: &mut TreePage| &mut page.appearance_controls.theme,
        )));
        children.push(Box::new(lens(
            move |state: &mut SelectState| {
                select(state, &modes.iter().map(String::as_str).collect::<Vec<_>>())
            },
            |page: &mut TreePage| &mut page.appearance_controls.mode,
        )));
        children.push(Box::new(lens(
            |input: &mut TextInput| textarea_typed(input).attr("aria-label", "Tabard theme JSON"),
            |page: &mut TreePage| &mut page.appearance_controls.input,
        )));
        children.push(Box::new(button(
            "Import theme",
            |page: &mut TreePage, _| {
                let mut session = page.shared.appearance.borrow_mut();
                if let Err(error) = session.import(page.appearance_controls.input.text()) {
                    web_sys::console::error_1(&error.into());
                }
                page.appearance_controls.sync(&session);
            },
        )));
    }
    Some(Box::new(
        el("section", children)
            .attr("class", "tree-appearance")
            .attr("aria-label", "Application appearance"),
    ))
}

pub(super) fn focused_text(
    runner: &cambium_rootstock::Runner<TreePage, Logic, Child>,
) -> Option<cambium_rootstock::FocusedTextSlot<TreePage>> {
    let node = runner.focus()?;
    let dom = runner.dom();
    let dom = dom.borrow();
    if runner.state().appearance_controls.open
        && taproot::matching(
            &dom,
            &Selector::role("textbox").containing("Tabard theme JSON"),
        )
        .contains(&node)
    {
        return Some(cambium_rootstock::FocusedTextSlot {
            node,
            get: Box::new(|page| &page.appearance_controls.input),
            get_mut: Box::new(|page| &mut page.appearance_controls.input),
        });
    }
    #[cfg(feature = "applets")]
    if taproot::matching(&dom, &Selector::class("applet-search-field")).contains(&node) {
        return Some(cambium_rootstock::FocusedTextSlot {
            node,
            get: Box::new(|page| &page.applet_query),
            get_mut: Box::new(|page| &mut page.applet_query),
        });
    }
    drop(dom);
    product::focused_text(runner)
}
