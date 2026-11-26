// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Wasm mod projects in the app: the section's actions and the refusals that
//! join the pack status view (mod projects MP5; D5).

use std::path::Path;

use super::App;
use crate::action::{Effect, ModProjectAction as Step};
use crate::command_packs::Level;
use crate::observe::AppEvent;

fn level(dev: bool) -> Level {
    if dev { Level::Dev } else { Level::Conformant }
}

impl App {
    /// The mod project lines of the pack status view: refusals and failures.
    pub(super) fn mod_status_rows(&self) -> Vec<crate::ui::Suggestion> {
        self.mods
            .iter()
            .flat_map(|mods| mods.status_lines())
            .filter(|line| !line.starts_with("No mod project"))
            .map(crate::ui::Suggestion::Prompt)
            .collect()
    }

    /// Run one project step; the outcome is an event, and a refusal opens the
    /// status view with its reason.
    pub(super) fn run_mod_project(&mut self, step: Step) -> Vec<Effect> {
        let Some(mods) = self.mods.clone() else {
            return vec![Effect::Redraw];
        };
        // A project attached to a page takes its turns through the shell's live page.
        if let Step::Turn { id, .. } = &step {
            if mods.page_binding(id).is_some() {
                return vec![Effect::ModProjectPage(step)];
            }
        }
        let (name, id, result): (&'static str, String, Result<String, String>) = match step {
            Step::Create { from } => {
                let from = from.map(std::path::PathBuf::from);
                let made = mods.create(from.as_deref());
                ("create", made.clone().unwrap_or_default(), made.map(|id| format!("ok: {id}")))
            },
            Step::Open { path, dev } => {
                let opened = mods.open_project(Path::new(&path), level(dev));
                ("open", opened.clone().unwrap_or(path), opened.map(|id| format!("ok: {id}")))
            },
            Step::Check(id) => {
                let result = mods.check(&id).map(|()| "ok: check passed".into());
                ("check", id, result)
            },
            Step::Build(id) => {
                let result = mods.build(&id).map(|()| "ok: building".into());
                ("build", id, result)
            },
            Step::Attach(id) => {
                let result = mods.attach(&id).map(|()| "ok: attached".into());
                ("attach", id, result)
            },
            // The page is the shell's: it resolves the focused Livery session.
            step @ Step::AttachPage(_) => return vec![Effect::ModProjectPage(step)],
            Step::RunFixtures(id) => {
                let result = mods.run_fixtures(&id);
                ("fixtures", id, result)
            },
            Step::Reload(id) => {
                let result = mods.reload(&id);
                ("reload", id, result)
            },
            Step::Rollback(id) => {
                let result = mods.rollback(&id);
                ("rollback", id, result)
            },
            Step::Detach(id) => {
                let result = mods.detach(&id).map(|()| "ok: detached".into());
                ("detach", id, result)
            },
            Step::SetLevel { id, dev } => {
                let result = mods.set_level(&id, level(dev)).map(|()| "ok: level set".into());
                ("level", id, result)
            },
            Step::SetGrant { id, capability, granted } => {
                let result = mods
                    .set_grant(&id, &capability, granted)
                    .map(|()| format!("ok: {capability} {}", if granted { "granted" } else { "revoked" }));
                ("grant", id, result)
            },
            Step::Turn { id, kind, payload } => {
                let result = mods.turn(&id, &kind, &payload);
                ("turn", id, result)
            },
        };
        self.finish_mod_project(name, id, result)
    }

    /// Record an outcome the shell found on its own (a page mod detached).
    pub(crate) fn record_mod_project_event(&mut self, step: &'static str, id: String, outcome: String) {
        self.events.push(AppEvent::ModProjectStep { step, id, outcome });
    }

    /// Record a step's outcome as an event; a refusal opens the status view.
    pub(crate) fn finish_mod_project(
        &mut self,
        name: &'static str,
        id: String,
        result: Result<String, String>,
    ) -> Vec<Effect> {
        let opens_status = result.is_err() && name != "fixtures" && name != "turn";
        let outcome = match result {
            Ok(note) if note.starts_with("ok") => note,
            Ok(note) => format!("ok: {note}"),
            Err(why) if name == "fixtures" => format!("failed: {why}"),
            Err(why) => format!("refused: {why}"),
        };
        self.events.push(AppEvent::ModProjectStep { step: name, id, outcome });
        if opens_status {
            return self.open_pack_status();
        }
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::mod_projects::ModProjectHost;
    use crate::ui::OmnibarMode;

    fn app() -> (App, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::test_stub();
        app.mods = Some(std::sync::Arc::new(ModProjectHost::open(dir.path())));
        (app, dir)
    }

    fn step(app: &mut App, step: Step) {
        app.update(Action::ModProject(step));
    }

    fn last(app: &App) -> String {
        app.events.iter().rev().find_map(|e| match e {
            AppEvent::ModProjectStep { step, id, outcome } => Some(format!("{step} {id} {outcome}")),
            _ => None,
        }).unwrap_or_default()
    }

    #[test]
    fn a_project_is_created_checked_attached_and_run_through_the_action_spine() {
        let (mut app, _dir) = app();
        step(&mut app, Step::Create { from: None });
        assert_eq!(last(&app), "create notes-mod ok: notes-mod");
        step(&mut app, Step::Check("notes-mod".into()));
        step(&mut app, Step::Attach("notes-mod".into()));
        step(&mut app, Step::RunFixtures("notes-mod".into()));
        assert_eq!(last(&app), "fixtures notes-mod ok: 1 of 1 fixtures passed");
        let rows = app.mods.as_ref().unwrap().listing().rows;
        assert!(rows[0].attached);
    }

    #[test]
    fn a_refusal_shows_in_the_status_view_by_name() {
        let (mut app, dir) = app();
        step(&mut app, Step::Create { from: None });
        let net = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mod-projects/alt/net.wasm");
        std::fs::copy(net, dir.path().join("mod-projects/notes-mod/component.wasm")).unwrap();
        step(&mut app, Step::Check("notes-mod".into()));
        assert!(last(&app).contains("refused: notes-mod: check failed: import mere:script/net is not granted"));
        assert!(app.omnibar.open);
        assert!(matches!(app.omnibar.mode, OmnibarMode::PackStatus));
        let rows = format!("{:?}", app.omnibar.suggestions);
        assert!(rows.contains("import mere:script/net is not granted"), "{rows}");
    }
}
