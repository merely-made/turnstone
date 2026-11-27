// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Mod projects section, beside Command packs (mod projects MP5; D5).
//! Per project: level, status, last check, build and trap, the document and
//! the last turn, fixture results and the guest's log, with the actions.

use super::*;
use crate::action::{Action, ModProjectAction as Step};
use crate::mod_projects::{Level, ProjectRow};

fn action_button(
    label: impl Into<String>,
    aria: String,
    action: impl Fn() -> Action + 'static,
) -> SettingsView {
    Box::new(
        button(
            label.into(),
            move |state: &mut SettingsState, _: cambium::PointerClick| {
                state.live_settings.request_action(action());
            },
        )
        .attr("aria-label", aria)
        .attr("class", "setting-apply pack-switch"),
    )
}

fn line(text: String) -> SettingsView {
    Box::new(el::<_, SettingsState, ()>("div", text).attr("class", "list-row muted"))
}

/// One line for a group of outline, diff or log lines (the pane flows its
/// rows, so a group is a single wrapped line).
fn group(heading: &str, body: &[String]) -> (String, SettingsView) {
    let joined: Vec<&str> = body.iter().map(|l| l.trim()).collect();
    (heading.to_string(), line(format!("{heading}: {}", joined.join(" | "))))
}

fn step_button(label: &'static str, id: &str, step: impl Fn(String) -> Step + 'static) -> (String, SettingsView) {
    let id = id.to_string();
    (
        label.to_string(),
        action_button(label, format!("{label} {id}"), move || {
            Action::ModProject(step(id.clone()))
        }),
    )
}

/// What kind of project it is and, for a parser, who reads which media type.
fn kind_text(row: &ProjectRow) -> String {
    if row.media_types.is_empty() {
        return format!("Kind: {}", row.kind);
    }
    let each: Vec<String> = row
        .media_types
        .iter()
        .map(|(media, claimed, on)| {
            let who = match (claimed, on) {
                (false, _) => "this parser",
                (true, true) => "this parser (handed over)",
                (true, false) => "a built-in engine",
            };
            format!("{media} by {who}")
        })
        .collect();
    format!("Kind: {}; {}", row.kind, each.join(", "))
}

/// The capabilities the manifest declares and whether each is granted.
fn grants_text(row: &ProjectRow) -> String {
    if row.capabilities.is_empty() {
        return "Grants: none declared in mod.toml".into();
    }
    let each: Vec<String> = row
        .capabilities
        .iter()
        .map(|(name, granted)| format!("{name} {}", if *granted { "granted" } else { "not granted" }))
        .collect();
    let live = if row.level == Level::Dev && row.capabilities.iter().any(|(_, granted)| *granted) {
        "; dev mode is unpinned, so a grant reaches whatever component.wasm holds (D21)"
    } else {
        ""
    };
    format!("Grants: {}; a change applies at the next Attach or Reload{live}", each.join(", "))
}

fn project(row: &ProjectRow) -> (String, SettingsView) {
    let id = &row.id;
    let dev = row.level.other() == Level::Dev;
    let mut buttons = vec![step_button("Check", id, Step::Check)];
    if row.can_build {
        buttons.push(step_button("Build", id, Step::Build));
    }
    if row.kind == "document" {
        if row.attached {
            buttons.push(step_button("Detach", id, Step::Detach));
        } else {
            buttons.push(step_button("Attach to its document", id, Step::Attach));
            buttons.push(step_button("Attach to the focused page", id, Step::AttachPage));
        }
    }
    if row.kind == "viewer" {
        buttons.push(step_button("View", id, Step::View));
    }
    for (media, claimed, on) in &row.media_types {
        if !claimed {
            continue;
        }
        let (id, media, now) = (id.clone(), media.clone(), !on);
        let label = format!("{} {media}", if *on { "Take back" } else { "Hand over" });
        buttons.push((
            format!("claim-{media}"),
            action_button(label.clone(), format!("{label} for {id}"), move || {
                Action::ModProject(Step::SetClaimed {
                    id: id.clone(),
                    media_type: media.clone(),
                    on: now,
                })
            }),
        ));
    }
    buttons.push(step_button("Run fixtures", id, Step::RunFixtures));
    buttons.push(step_button("Reload", id, Step::Reload));
    if row.can_rollback {
        buttons.push(step_button("Roll back", id, Step::Rollback));
    }
    for (name, granted) in &row.capabilities {
        let (id, cap, now) = (id.clone(), name.clone(), !granted);
        buttons.push((
            format!("grant-{name}"),
            action_button(
                format!("{} {name}", if *granted { "Revoke" } else { "Grant" }),
                format!("{} {name} for {id}", if *granted { "Revoke" } else { "Grant" }),
                move || {
                    Action::ModProject(Step::SetGrant {
                        id: id.clone(),
                        capability: cap.clone(),
                        granted: now,
                    })
                },
            ),
        ));
    }
    let level_id = id.clone();
    buttons.push((
        "level".into(),
        action_button(
            if dev { "Use dev mode" } else { "Use conformant" },
            format!("Set {id} admission level"),
            move || Action::ModProject(Step::SetLevel { id: level_id.clone(), dev }),
        ),
    ));
    buttons.push(step_button("Uninstall", id, Step::Uninstall));
    let mut parts = vec![
        (
            "name".to_string(),
            line(format!("{id} \u{2014} {} \u{2014} {}", row.level.label(), row.status)),
        ),
        ("kind".into(), line(kind_text(row))),
        ("check".into(), line(format!("Last check: {}", row.check))),
        ("build".into(), line(format!("Last build: {}", row.build))),
        ("trap".into(), line(format!("Last trap: {}", row.trap))),
        ("grants".into(), line(grants_text(row))),
        ("changes".into(), line(row.changes.clone())),
    ];
    if !row.component.is_empty() {
        parts.push(("component".into(), line(row.component.clone())));
    }
    for (n, text) in row.diff.iter().enumerate() {
        parts.push((format!("diff-{n}"), line(format!("Diff {text}"))));
    }
    if !row.document.is_empty() {
        parts.push(group("Document", &row.document));
    }
    if let Some(turn) = &row.last_turn {
        parts.push(("turn".into(), line(format!("Last turn: {turn}"))));
    }
    if !row.fixtures.is_empty() {
        parts.push(group("Fixtures", &row.fixtures));
    }
    if !row.logs.is_empty() {
        parts.push(group("Log", &row.logs));
    }
    if let Some(failure) = &row.failure {
        parts.push(("failure".into(), line(format!("Last failure: {failure}"))));
    }
    parts.push((
        "actions".into(),
        Box::new(el::<_, SettingsState, ()>("div", Keyed::new(buttons))) as SettingsView,
    ));
    (
        row.id.clone(),
        Box::new(
            el::<_, SettingsState, ()>("div", Keyed::new(parts))
                .attr("class", "pack-switch-row")
                .attr("aria-label", format!("Mod project {id}")),
        ),
    )
}

fn available(path: &std::path::Path) -> (String, SettingsView) {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let text = path.to_string_lossy().into_owned();
    let (conformant, dev) = (text.clone(), text);
    (
        name.clone(),
        Box::new(
            el::<_, SettingsState, ()>(
                "div",
                Keyed::new(vec![
                    ("name".to_string(), line(format!("Not opened: {name}"))),
                    (
                        "open".into(),
                        action_button("Open", format!("Open {name}"), move || {
                            Action::ModProject(Step::Open { path: conformant.clone(), dev: false })
                        }),
                    ),
                    (
                        "dev".into(),
                        action_button("Open in dev mode", format!("Open {name} in dev mode"), move || {
                            Action::ModProject(Step::Open { path: dev.clone(), dev: true })
                        }),
                    ),
                ]),
            )
            .attr("class", "pack-switch-row"),
        ),
    )
}

pub(super) fn view(state: &SettingsState) -> SettingsView {
    let listing = state.live_settings.mod_projects();
    let top = Keyed::new(vec![
        (
            "create",
            action_button("Create from starter", "Create a mod project from the starter".into(), || {
                Action::ModProject(Step::Create { from: None })
            }),
        ),
        (
            "status",
            action_button("Show refusals", "Show mod project refusals".into(), || {
                Action::ShowPackStatus
            }),
        ),
    ]);
    Box::new(
        el::<_, SettingsState, ()>(
            "section",
            (
                el::<_, SettingsState, ()>("div", "Mod projects").attr("class", "list-section-title"),
                el::<_, SettingsState, ()>("div", top),
                el::<_, SettingsState, ()>("div", Keyed::new(listing.rows.iter().map(project).collect::<Vec<_>>())),
                el::<_, SettingsState, ()>(
                    "div",
                    Keyed::new(listing.available.iter().map(|p| available(p)).collect::<Vec<_>>()),
                ),
            ),
        )
        .attr("aria-label", "Mod projects"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_projects::{ProjectListing, ProjectRow};

    #[test]
    fn the_section_shows_a_project_and_offers_its_actions() {
        let root = tempfile::tempdir().unwrap();
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        live.publish_mod_projects(ProjectListing {
            rows: vec![ProjectRow {
                id: "notes-mod".into(),
                title: "Notes mod".into(),
                level: Level::Conformant,
                status: "Not attached".into(),
                check: "check passed".into(),
                build: "not built".into(),
                trap: "none".into(),
                document: Vec::new(),
                last_turn: None,
                fixtures: vec!["set: pass".into()],
                logs: Vec::new(),
                failure: Some("check failed: import mere:script/net is not granted".into()),
                attached: false,
                can_build: true,
                can_rollback: true,
                changes: "Changed since the last pass: 0 files".into(),
                component: "component.wasm matches last-good.wasm".into(),
                diff: Vec::new(),
                capabilities: vec![("network".into(), false)],
                kind: "document".into(),
                media_types: Vec::new(),
            }],
            available: vec![root.path().join("other")],
        });
        pane.sync(640.0, 780.0);
        let button = |name: &str| taproot::Selector::role("button").with_attr("aria-label", name);
        for name in [
            "Check notes-mod",
            "Build notes-mod",
            "Attach to its document notes-mod",
            "Attach to the focused page notes-mod",
            "Run fixtures notes-mod",
            "Reload notes-mod",
            "Roll back notes-mod",
            "Uninstall notes-mod",
            "Grant network for notes-mod",
            "Open other",
            "Create a mod project from the starter",
        ] {
            let found = taproot::matching_estimated_fixture(&pane.dom_ref(), &button(name));
            assert!(!found.is_empty(), "{name}");
        }
        let check = taproot::matching_estimated_fixture(&pane.dom_ref(), &button("Check notes-mod"));
        let _: Vec<()> = pane
            .runner
            .dispatch_click(check[0], cambium::PointerClick::at((0.0, 0.0)));
        let queued = format!("{:?}", live.take_actions());
        assert!(queued.contains("Check(\"notes-mod\")"), "{queued}");
    }
}
