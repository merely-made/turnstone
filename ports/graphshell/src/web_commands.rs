// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The page's command context menu (Scenograph editor plan, track C1).
//!
//! A right click on the canvas opens it at the pointer: a search field, then
//! the commands for what was clicked, the person's kept commands and the
//! recent ones (SE28). Cambium's [`CommandSet`] composes the rows; the page
//! draws them as DOM like its panels (SE33). What the person keeps and has
//! used is a view of Graphshell's in the mere session (SE31).

use cambium::{Command, CommandChoices, CommandSet, MenuSession};
use graphshell::mere_host::{GRAPHSHELL, SessionViewIntent as ViewIntent};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, HtmlInputElement};

use super::web_session::SessionStore;
use super::{BrowserHost, document, element};

/// The view the menu's choices are kept in.
const COMMANDS_VIEW: &str = "commands";

/// The commands the page offers through its menu, by surface.
pub(super) fn page_commands() -> CommandSet {
    let mut set = CommandSet::new().with_defaults([
        "zoom-in",
        "zoom-out",
        "fit-content",
        "add-address",
        "open-projection-editor",
        "session-undo",
        "session-redo",
        "save-scene",
    ]);
    for (id, label, category) in [
        ("open-detail", "Edit selected object", "node"),
        ("invoke-action", "Open selected object", "node"),
        ("select-web", "Select research object", "canvas"),
        ("zoom-in", "Zoom in", "canvas"),
        ("zoom-out", "Zoom out", "canvas"),
        ("fit-content", "Fit to view", "canvas"),
        ("pan-left", "Pan left", "canvas"),
        ("pan-right", "Pan right", "canvas"),
        ("pan-up", "Pan up", "canvas"),
        ("pan-down", "Pan down", "canvas"),
        ("add-address", "Add address", "canvas"),
        ("toggle-physics", "Pause or resume physics", "canvas"),
        ("session-undo", "Undo change", "session"),
        ("session-redo", "Redo change", "session"),
        ("session-local", "Show the local mere", "session"),
        ("session-remote", "Show the remote mount", "session"),
        ("save-scene", "Save scene", "session"),
        ("reopen-scene", "Reopen saved scene", "session"),
        ("export-codicil", "Export codicil", "session"),
        ("open-codicil", "Open exported codicil", "session"),
        ("open-projection-editor", "Edit projection", "projection"),
        (
            "close-projection-editor",
            "Close projection editor",
            "projection",
        ),
        ("save-projection", "Save projection", "projection"),
        ("reload-projection", "Reload saved projection", "projection"),
        ("undo-projection", "Undo projection edit", "projection"),
        ("redo-projection", "Redo projection edit", "projection"),
        ("undo-projection-save", "Undo projection save", "projection"),
        ("redo-projection-save", "Redo projection save", "projection"),
        (
            "load-practice-projection",
            "Open practice projection",
            "projection",
        ),
        (
            "reopen-practice-projection",
            "Reopen saved practice projection",
            "projection",
        ),
        ("projection-grid", "Projection: grid", "projection"),
        (
            "projection-scatter",
            "Projection: order by tempo",
            "projection",
        ),
    ] {
        set.register(Command::new(id, label, category));
    }
    set
}

/// The menu while it is open.
pub(super) struct OpenMenu {
    /// Client px.
    at: (i32, i32),
    /// Its query, context (`"node"`, or `None` on empty canvas, where only the
    /// kept and recent commands show, SE29) and search focus.
    session: MenuSession,
    /// The rows changed since they were last drawn.
    stale: bool,
}

/// The person's choices as the session keeps them.
pub(super) fn stored_choices(
    host: &graphshell::mere_host::MereHost<muniment::IndexedDbBackend>,
) -> CommandChoices {
    host.view(GRAPHSHELL, COMMANDS_VIEW)
        .and_then(|view| view.commands.clone())
        .unwrap_or_default()
}

impl BrowserHost {
    /// Open the menu at client `(x, y)`, over `node` when one was clicked.
    pub(super) fn open_command_menu(&mut self, at: (i32, i32), node: Option<uuid::Uuid>) {
        if let Some(member) = node {
            // Node commands act on the selection, so a right click selects
            // the node under it first (Turnstone's rule).
            self.canvas.select_member(member);
            self.primary_member = Some(member);
        }
        self.command_menu = Some(OpenMenu {
            at,
            session: MenuSession::open(node.map(|_| "node")),
            stale: true,
        });
        self.chrome_dirty = true;
    }

    pub(super) fn close_command_menu(&mut self) {
        if self.command_menu.take().is_some() {
            self.chrome_dirty = true;
        }
    }

    pub(super) fn command_menu_open(&self) -> bool {
        self.command_menu.is_some()
    }

    pub(super) fn search_commands(&mut self, query: &str) {
        if let Some(menu) = self.command_menu.as_mut() {
            menu.session.set_query(query);
            menu.stale = true;
        }
    }

    /// What a row asked for: `run:<id>`, `keep:<id>` or `drop:<id>`.
    pub(super) fn command_menu_action(&mut self, action: &str) {
        let Some((verb, id)) = action.split_once(':') else {
            return;
        };
        match verb {
            "run" => {
                self.close_command_menu();
                self.command_set.record_use(&mut self.command_choices, id);
                self.keep_command_choices();
                self.run_command(id);
            },
            "keep" => {
                self.command_set.add(&mut self.command_choices, id);
                self.keep_command_choices();
            },
            "drop" => {
                self.command_set.remove(&mut self.command_choices, id);
                self.keep_command_choices();
            },
            _ => {},
        }
        if let Some(menu) = self.command_menu.as_mut() {
            menu.stale = true;
        }
    }

    /// Run the first row the menu shows, as Enter in its search field does.
    pub(super) fn run_first_command(&mut self) {
        let chosen = self.command_menu.as_ref().and_then(|menu| {
            let rows = menu.session.rows(&self.command_set, &self.command_choices);
            menu.session.chosen(&rows).map(|command| command.id.clone())
        });
        if let Some(id) = chosen {
            self.command_menu_action(&format!("run:{id}"));
        }
    }

    /// Keep the person's choices in the session; the frame pump stores them.
    fn keep_command_choices(&mut self) {
        let view = ViewIntent {
            commands: Some(self.command_choices.clone()),
            ..ViewIntent::default()
        };
        match self.app.host.set_view_now(GRAPHSHELL, COMMANDS_VIEW, view) {
            Ok(()) => self.session_store = SessionStore::Pending,
            Err(error) => self.product_status = format!("Command menu not kept · {error}"),
        }
    }
}

/// Draw the menu when it opened, closed or changed.
pub(super) fn present_command_menu(host: &mut BrowserHost) -> Result<(), String> {
    let menu_element = element("command-menu")?;
    let Some(menu) = host.command_menu.as_mut() else {
        if !menu_element.has_attribute("hidden") {
            menu_element
                .set_attribute("hidden", "")
                .map_err(|_| "could not hide the command menu")?;
        }
        return Ok(());
    };
    if !menu.stale && !menu.session.focus_search {
        return Ok(());
    }
    let items = menu.session.rows(&host.command_set, &host.command_choices);
    let kept: Vec<String> = host
        .command_set
        .kept(&host.command_choices)
        .into_iter()
        .map(|command| command.id.clone())
        .collect();
    let list = element("command-menu-items")?;
    list.set_inner_html("");
    let document = document()?;
    if items.is_empty() {
        let empty = document
            .create_element("p")
            .map_err(|_| "could not draw the command menu")?;
        empty.set_text_content(Some("No command matches"));
        list.append_child(&empty)
            .map_err(|_| "could not draw the command menu")?;
    }
    for item in &items {
        let row = row(&document, item, kept.contains(&item.id))?;
        list.append_child(&row)
            .map_err(|_| "could not draw the command menu")?;
    }
    menu_element
        .remove_attribute("hidden")
        .map_err(|_| "could not show the command menu")?;
    // At the pointer, kept inside the window: shown first, so it has a size.
    let window = web_sys::window().ok_or("no window")?;
    let inner = |value: Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>| {
        value.ok().and_then(|value| value.as_f64()).unwrap_or(0.0)
    };
    let (width, height) = (inner(window.inner_width()), inner(window.inner_height()));
    let rect = menu_element.get_bounding_client_rect();
    let left = f64::from(menu.at.0)
        .min(width - rect.width() - 8.0)
        .max(8.0);
    let top = f64::from(menu.at.1)
        .min(height - rect.height() - 8.0)
        .max(8.0);
    if let Some(style) = menu_element
        .dyn_ref::<HtmlElement>()
        .map(HtmlElement::style)
    {
        let _ = style.set_property("left", &format!("{left}px"));
        let _ = style.set_property("top", &format!("{top}px"));
    }
    let search: HtmlInputElement = element("command-search")?
        .dyn_into()
        .map_err(|_| "the command search is not an input")?;
    if menu.session.focus_search {
        search.set_value("");
        let _ = search.focus();
        menu.session.focus_search = false;
    }
    menu.stale = false;
    Ok(())
}

/// One row: the command, and a control to keep it or stop keeping it.
fn row(document: &web_sys::Document, item: &Command, kept: bool) -> Result<Element, String> {
    let make = |tag: &str| {
        document
            .create_element(tag)
            .map_err(|_| "could not draw the command menu".to_string())
    };
    let row = make("div")?;
    row.set_class_name("command-row");
    let run = make("button")?;
    run.set_attribute("type", "button").ok();
    run.set_attribute("role", "menuitem").ok();
    run.set_attribute("data-command-menu", &format!("run:{}", item.id))
        .ok();
    run.set_text_content(Some(&item.label));
    if item.disabled_reason.is_some() {
        run.set_attribute("disabled", "").ok();
        if let Some(reason) = &item.disabled_reason {
            run.set_attribute("title", reason).ok();
        }
    }
    let toggle = make("button")?;
    toggle.set_attribute("type", "button").ok();
    toggle.set_class_name("command-keep");
    let (verb, mark, label) = if kept {
        (
            "drop",
            "\u{2212}",
            format!("Remove {} from the menu", item.label),
        )
    } else {
        ("keep", "+", format!("Keep {} in the menu", item.label))
    };
    toggle
        .set_attribute("data-command-menu", &format!("{verb}:{}", item.id))
        .ok();
    toggle.set_attribute("aria-label", &label).ok();
    toggle.set_attribute("title", &label).ok();
    toggle.set_text_content(Some(mark));
    row.append_child(&run)
        .map_err(|_| "could not draw the command menu")?;
    row.append_child(&toggle)
        .map_err(|_| "could not draw the command menu")?;
    Ok(row)
}
