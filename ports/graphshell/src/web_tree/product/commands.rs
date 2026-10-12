// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Command choices are session data; the retained menu is a shared widget view.
use super::*;
use cambium::{CommandChoices, CommandSet, MenuSession, button, lens, on_key, text_field_typed};
use graphshell::mere_host::{GRAPHSHELL, SessionViewIntent};

pub(super) struct Commands {
    set: CommandSet,
    choices: CommandChoices,
    menu: Option<(MenuSession, (f32, f32))>,
    pub(super) query: TextInput,
}
impl Commands {
    pub(super) fn load(app: &App) -> Self {
        Self {
            set: crate::web_command_catalog::page_commands(),
            choices: app
                .host
                .view(GRAPHSHELL, "commands")
                .and_then(|view| view.commands.clone())
                .unwrap_or_default(),
            menu: None,
            query: TextInput::new(""),
        }
    }
}
impl SavedProduct {
    pub(in crate::web_tree) fn menu_open(&self) -> bool {
        self.commands.menu.is_some()
    }
    pub(in crate::web_tree) fn close_menu(&mut self) {
        self.commands.menu = None;
    }
    pub(in crate::web_tree) fn open_menu(
        &mut self,
        request: mere::canvas::ContextRequest,
        canvas: &mut Canvas,
    ) {
        if self.selection_locked() {
            return;
        }
        if let Some(member) = request.node {
            canvas.select_member(member);
            self.select(Some(member));
        }
        self.commands.menu = Some((MenuSession::open(request.node.map(|_| "node")), request.at));
        self.commands.query = TextInput::new("");
    }
    fn menu_action(&mut self, action: &str) -> Result<Option<String>, String> {
        if self.selection_locked() {
            return Err("A save is pending".into());
        }
        let (verb, id) = action.split_once(':').ok_or("Unknown menu action")?;
        let c = &mut self.commands;
        match verb {
            "keep" => {
                c.set.add(&mut c.choices, id);
            },
            "drop" => {
                c.set.remove(&mut c.choices, id);
            },
            "run" => {
                c.set.record_use(&mut c.choices, id);
                c.menu = None;
            },
            _ => return Err("Unknown menu action".into()),
        }
        self.app
            .as_mut()
            .ok_or("Local graph is busy")?
            .host
            .set_view_now(
                GRAPHSHELL,
                "commands",
                SessionViewIntent {
                    commands: Some(c.choices.clone()),
                    ..SessionViewIntent::default()
                },
            )
            .map_err(|e| e.to_string())?;
        Ok((verb == "run").then(|| id.to_owned()))
    }
    pub(in crate::web_tree) fn run_command(
        &mut self,
        id: &str,
        canvas: &mut Canvas,
        layout: &str,
        viewport: (u32, u32),
    ) -> Result<(), String> {
        match id {
            "node:edit" => {
                if self.selected.is_none() {
                    canvas.select_by_url(FIXTURE_WEB_ADDRESS);
                    self.select(canvas.focused_member());
                }
                self.open_detail(canvas);
            },
            "invoke-action" => self.open_selected(),
            "node:new" => {
                self.intake_open = true;
                if !self.intake_address.text().trim().is_empty() {
                    self.add_address();
                }
            },
            "select-web" => {
                canvas.select_by_url(FIXTURE_WEB_ADDRESS);
                self.select(canvas.focused_member());
            },
            "open-projection-editor" => self.projection.open = true,
            "close-projection-editor" => self.projection.open = false,
            "projection-grid" | "projection-scatter" => {
                self.projection_action(if id == "projection-grid" {
                    "grid"
                } else {
                    "scatter"
                })?
            },
            "load-practice-projection" => self.projection_action("load")?,
            "reopen-practice-projection" => {
                self.projection_action("load")?;
                self.projection_action("reopen")?;
            },
            "save-projection" => self.projection_action("save")?,
            "reload-projection" => self.projection_action("reopen")?,
            "undo-projection" => self.projection_action("undo")?,
            "redo-projection" => self.projection_action("redo")?,
            "undo-projection-save" => self.projection_action("undo-save")?,
            "redo-projection-save" => self.projection_action("redo-save")?,
            "compare-projection" => self.projection_action("compare")?,
            "compare-projection-dynamics" => self.projection_action("compare-dynamics")?,
            "projection-motion" => self.projection_action("motion")?,
            "projection-compare-left" => self.projection_action("compare-left")?,
            "projection-compare-right" => self.projection_action("compare-right")?,
            "projection-compare-up" => self.projection_action("compare-up")?,
            "projection-compare-down" => self.projection_action("compare-down")?,
            "save-scene" => self.scene_action(canvas, layout, viewport, "save")?,
            "reopen-scene" => self.scene_action(canvas, layout, viewport, "reopen")?,
            "export-codicil" => self.scene_action(canvas, layout, viewport, "export")?,
            "open-codicil" => self.scene_action(canvas, layout, viewport, "open")?,
            "session:undo" | "session:redo" => {
                let app = self.app.as_mut().ok_or("Local graph is busy")?;
                let back = id == "session:undo";
                let reverted = if back {
                    app.host.undo_now(GRAPHSHELL)
                } else {
                    app.host.redo_now(GRAPHSHELL)
                }
                .map_err(|e| e.to_string())?;
                if let Some(reverted) = reverted {
                    self.forme_reconcile_session = Some(app.host.graph_session().id().0);
                    canvas.set_graph(app.host.graph().clone());
                    self.status = if back {
                        "Undid Graphshell's last change"
                    } else {
                        "Redid Graphshell's last change"
                    }
                    .into();
                    if let Some(kept) = graphshell::product::kept_summary(&reverted.kept) {
                        self.status.push_str(&format!(" · {kept}"));
                    }
                    self.operation_success = Some(self.status.clone());
                    self.persist_operation();
                } else {
                    self.status = "Nothing to undo or redo in the session".into();
                }
            },
            _ => return Err(format!("Unknown command: {id}")),
        }
        Ok(())
    }
}
pub(in crate::web_tree) fn menu(page: &TreePage) -> Option<Child> {
    let product = page.product.as_ref()?;
    let c = &product.commands;
    let (session, at) = c.menu.as_ref()?;
    let mut session = session.clone();
    session.set_query(c.query.text());
    let rows = session.rows(&c.set, &c.choices);
    let kept = c.set.kept(&c.choices);
    let mut children: Vec<Child> = vec![Box::new(lens(
        |input: &mut TextInput| text_field_typed(input).attr("aria-label", "Search commands"),
        |page: &mut TreePage| &mut page.product.as_mut().expect("menu").commands.query,
    ))];
    for item in rows {
        for (verb, label) in [
            ("run", item.label.clone()),
            if kept.iter().any(|kept| kept.id == item.id) {
                ("drop", format!("Remove {} from the menu", item.label))
            } else {
                ("keep", format!("Keep {} in the menu", item.label))
            },
        ] {
            let action = format!("{verb}:{}", item.id);
            let target = action.clone();
            children.push(Box::new(
                button(label, move |page: &mut TreePage, _| {
                    let result = page
                        .product
                        .as_mut()
                        .ok_or("Local graph is unavailable".to_owned())
                        .and_then(|product| product.menu_action(&action));
                    match result {
                        Ok(command) => {
                            if let Some(command) = command {
                                page.command(&command);
                            }
                            if let Some(product) = &mut page.product {
                                product.persist_operation();
                            }
                        },
                        Err(error) => {
                            if let Some(product) = &mut page.product {
                                product.status = error;
                            }
                        },
                    }
                    page.shared.dirty.set(true);
                })
                .attr("data-command-menu", target),
            ));
        }
    }
    if children.len() == 1 {
        children.push(Box::new(el("p", "No command matches")));
    }
    children.push(Box::new(button(
        "Close commands",
        |page: &mut TreePage, _| {
            if let Some(product) = &mut page.product {
                product.close_menu();
            }
        },
    )));
    let x = at.0.min((page.size.0 as f32 - 340.0).max(8.0));
    let y = at.1.min((page.size.1 as f32 - 360.0).max(8.0));
    Some(Box::new(on_key(el("section",children).attr("aria-label","Graph commands").attr("class","tree-command-menu")
        .attr("style",format!("position:absolute;left:{x}px;top:{y}px;width:320px;max-height:350px;overflow:auto;z-index:12;background:#1e303d;padding:10px;")),
        |page: &mut TreePage,event: cambium::KeyEvent| {
            if matches!(event.key, Key::Named(NamedKey::Escape)) {
                if let Some(product) = &mut page.product { product.close_menu(); }
                event.prevent_default();
            }
        })))
}
