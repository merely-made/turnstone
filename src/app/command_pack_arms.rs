// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Command packs in the app: the catalog's pack rows, running a pack command
//! and the manager's actions (command packs CP4; D13 to D15).

use command_packs::CommandContext;

use super::{App, palette::CONTEXT_COMMANDS};
use crate::action::{Action, Effect};
use crate::command_packs::{APP_TARGET, Level, PACK_CATEGORY};
use crate::observe::AppEvent;
use crate::ui::{OmnibarMode, OmnibarState};

fn level(dev: bool) -> Level {
    if dev { Level::Dev } else { Level::Conformant }
}

impl App {
    /// The catalog rows for the pack commands listed on the app target.
    pub(crate) fn pack_rows(&self) -> Vec<(String, Action)> {
        let Some(packs) = &self.packs else {
            return Vec::new();
        };
        packs
            .commands_for(APP_TARGET)
            .into_iter()
            .map(|command| {
                let id = command.id;
                (command.label, Action::RunPackCommand { id })
            })
            .collect()
    }

    /// The registered declaration of pack command `id`.
    pub(crate) fn pack_command(&self, id: &str) -> Option<cambium::Command> {
        let mut command = self
            .packs
            .as_ref()?
            .commands_for(APP_TARGET)
            .into_iter()
            .find(|command| command.id == id)?;
        command.category = PACK_CATEGORY.into();
        Some(command)
    }

    /// What a script reads as `context`: the focused graph member, if any.
    fn pack_context(&self) -> CommandContext {
        let observed = self.command_observation();
        let mut context = CommandContext::new();
        if let Some(member) = observed.member {
            context = context.with_selection([member.to_string()]);
        }
        if let Some(graph) = observed.graph {
            context = context.with_focus("graph", Some(format!("{graph:?}")));
        }
        context
    }

    /// The rows of the pack status view, bounded to `row_limit`.
    pub(super) fn pack_status_rows(&self, row_limit: usize) -> Vec<crate::ui::Suggestion> {
        use crate::ui::Suggestion;
        let mut rows = vec![Suggestion::Hint("Command pack and mod project refusals and failed reloads")];
        rows.extend(
            self.packs
                .iter()
                .flat_map(|packs| packs.status_lines())
                .map(Suggestion::Prompt),
        );
        rows.extend(self.mod_status_rows());
        rows.truncate(row_limit.max(1));
        rows
    }

    pub(super) fn open_pack_status(&mut self) -> Vec<Effect> {
        let mut effects = self.cancel_smolweb_conversation();
        let target = self.fallback_shell_context();
        self.shell.begin_omnibar(target);
        self.recall.clear();
        self.recall_query.clear();
        self.omnibar = OmnibarState {
            open: true,
            mode: OmnibarMode::PackStatus,
            ..OmnibarState::default()
        };
        self.focus = crate::surface::FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        self.events.push(AppEvent::OmnibarOpened);
        effects.push(Effect::Redraw);
        effects
    }

    /// Run pack command `id` and dispatch each request it returns through the
    /// ordinary action path. A refusal shows its reason in the pack status view.
    pub(super) fn run_pack_command(&mut self, id: String) -> Vec<Effect> {
        let Some(packs) = self.packs.clone() else {
            return vec![Effect::Redraw];
        };
        packs.offer(&self.command_catalog().set);
        let invocations = match packs.run(&id, &self.pack_context()) {
            Ok(invocations) => invocations,
            Err(reason) => {
                self.events.push(AppEvent::PackRefused(reason));
                return self.open_pack_status();
            },
        };
        let mut effects = Vec::new();
        for invocation in &invocations {
            let catalog = self.command_catalog();
            let refusal = match catalog.set.get(&invocation.id) {
                None => Some("is no longer offered".to_string()),
                Some(command) if command.category == CONTEXT_COMMANDS => {
                    Some("is a context row, not a command".to_string())
                },
                Some(command) if command.category == PACK_CATEGORY => {
                    Some("is a pack command; packs do not run packs".to_string())
                },
                Some(command) => command.disabled_reason.clone(),
            };
            let action = catalog.action(&invocation.id).cloned();
            match (refusal, action) {
                (None, Some(action)) => effects.extend(self.update(action)),
                (reason, _) => {
                    let line = format!(
                        "{id} refused: request {} {}",
                        invocation.id,
                        reason.unwrap_or_else(|| "has no action".into())
                    );
                    packs.note_refusal(line.clone());
                    self.events.push(AppEvent::PackRefused(line));
                    effects.extend(self.open_pack_status());
                    return effects;
                },
            }
        }
        self.events.push(AppEvent::PackRan {
            id,
            requests: invocations.len(),
        });
        effects.push(Effect::Redraw);
        effects
    }

    fn pack_outcome(&mut self, result: Result<(), String>) -> Vec<Effect> {
        if let Err(reason) = result {
            self.events.push(AppEvent::PackRefused(reason));
        }
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    pub(super) fn install_command_pack(&mut self, path: String, dev: bool) -> Vec<Effect> {
        let result = self.packs.as_ref().map(|packs| {
            packs
                .install(std::path::Path::new(&path), level(dev))
                .map(drop)
        });
        self.pack_outcome(result.unwrap_or(Ok(())))
    }

    pub(super) fn reload_command_pack(&mut self, id: String) -> Vec<Effect> {
        let result = self.packs.as_ref().map(|packs| packs.reload(&id));
        self.pack_outcome(result.unwrap_or(Ok(())))
    }

    pub(super) fn unload_command_pack(&mut self, id: String) -> Vec<Effect> {
        let result = self.packs.as_ref().map(|packs| packs.unload(&id));
        self.pack_outcome(result.unwrap_or(Ok(())))
    }

    pub(super) fn set_command_pack_level(&mut self, id: String, dev: bool) -> Vec<Effect> {
        let result = self
            .packs
            .as_ref()
            .map(|packs| packs.set_level(&id, level(dev)));
        self.pack_outcome(result.unwrap_or(Ok(())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_packs::{PackHost, tests::copy_fixture};

    fn app_with_pack() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let root = copy_fixture(dir.path());
        let mut app = App::test_stub();
        let host = PackHost::open(dir.path());
        host.install(&root, Level::Conformant).unwrap();
        app.packs = Some(std::sync::Arc::new(host));
        (app, dir)
    }

    #[test]
    fn pack_commands_join_the_catalog_under_their_own_ids() {
        let (app, _dir) = app_with_pack();
        assert!(
            app.available_actions()
                .iter()
                .any(|(label, _)| label == "Demo: fit the view")
        );
        let catalog = app.command_catalog();
        let command = catalog.set.get("demo-pack:fit").unwrap();
        assert_eq!(command.category, PACK_CATEGORY);
        assert_eq!(catalog.id_of("Demo: fit the view"), Some("demo-pack:fit"));
        // The host's own rows are unchanged.
        assert!(catalog.set.get(cambium::catalogue::ids::VIEW_FIT).is_some());
    }

    #[test]
    fn a_granted_run_dispatches_and_a_refused_one_shows_its_reason() {
        let (mut app, _dir) = app_with_pack();
        app.update(Action::RunPackCommand {
            id: "demo-pack:fit".into(),
        });
        assert!(app.events.iter().any(|e| matches!(e, AppEvent::PackRan { requests: 1, .. })));
        app.update(Action::RunPackCommand {
            id: "demo-pack:reload".into(),
        });
        assert!(app.omnibar.open);
        assert!(matches!(app.omnibar.mode, OmnibarMode::PackStatus));
        let rows = format!("{:?}", app.omnibar.suggestions);
        assert!(rows.contains("not granted nav:reload"), "{rows}");
    }
}
