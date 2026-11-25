// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The Command packs section, beside the engine pack's (command packs D15).
//! The admission level shows here and nowhere else (graph-behaviors D6).

use super::*;
use crate::action::Action;

fn action_button(
    label: &'static str,
    aria: String,
    action: impl Fn() -> Action + 'static,
) -> SettingsView {
    Box::new(
        button(
            label,
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

pub(super) fn view(state: &SettingsState) -> SettingsView {
    let listing = state.live_settings.command_packs();
    if listing.rows.is_empty() && listing.available.is_empty() {
        return Box::new(el::<_, SettingsState, ()>("div", ()));
    }
    let rows = Keyed::new(
        listing
            .rows
            .iter()
            .map(|row| {
                let (id, dev) = (row.id.clone(), row.level.other() == crate::command_packs::Level::Dev);
                let buttons = Keyed::new(vec![
                    (
                        "reload",
                        action_button("Reload", format!("Reload {id}"), {
                            let id = id.clone();
                            move || Action::ReloadCommandPack { id: id.clone() }
                        }),
                    ),
                    (
                        "unload",
                        action_button("Unload", format!("Unload {id}"), {
                            let id = id.clone();
                            move || Action::UnloadCommandPack { id: id.clone() }
                        }),
                    ),
                    (
                        "level",
                        action_button(
                            if dev {
                                "Use dev mode"
                            } else {
                                "Use conformant"
                            },
                            format!("Set {id} admission level"),
                            {
                                let id = id.clone();
                                move || Action::SetCommandPackLevel {
                                    id: id.clone(),
                                    dev,
                                }
                            },
                        ),
                    ),
                ]);
                let mut parts = vec![
                    (
                        "name",
                        line(format!(
                            "{} \u{2014} {} \u{2014} {}",
                            row.id,
                            row.level.label(),
                            row.status
                        )),
                    ),
                    ("commands", line(format!("Commands: {}", row.commands.join("; ")))),
                    ("grants", line(format!("Grants: {}", row.grants))),
                ];
                if let Some(failure) = &row.last_failure {
                    parts.push(("failure", line(format!("Last failed reload: {failure}"))));
                }
                parts.push((
                    "actions",
                    Box::new(el::<_, SettingsState, ()>("div", buttons)) as SettingsView,
                ));
                (
                    row.id.clone(),
                    Box::new(
                        el::<_, SettingsState, ()>("div", Keyed::new(parts))
                            .attr("class", "pack-switch-row")
                            .attr("aria-label", format!("Command pack {}", row.id)),
                    ) as SettingsView,
                )
            })
            .collect::<Vec<_>>(),
    );
    let available = Keyed::new(
        listing
            .available
            .iter()
            .map(|path| {
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
                                ("name", line(format!("Not installed: {name}"))),
                                (
                                    "conformant",
                                    action_button("Install", format!("Install {name}"), move || {
                                        Action::InstallCommandPack {
                                            path: conformant.clone(),
                                            dev: false,
                                        }
                                    }),
                                ),
                                (
                                    "dev",
                                    action_button(
                                        "Install in dev mode",
                                        format!("Install {name} in dev mode"),
                                        move || Action::InstallCommandPack {
                                            path: dev.clone(),
                                            dev: true,
                                        },
                                    ),
                                ),
                            ]),
                        )
                        .attr("class", "pack-switch-row"),
                    ) as SettingsView,
                )
            })
            .collect::<Vec<_>>(),
    );
    Box::new(
        el::<_, SettingsState, ()>(
            "section",
            (
                el::<_, SettingsState, ()>("div", "Command packs")
                    .attr("class", "list-section-title"),
                el::<_, SettingsState, ()>("div", rows),
                el::<_, SettingsState, ()>("div", available),
            ),
        )
        .attr("aria-label", "Command packs"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_packs::{Level, PackListing, PackRow};

    #[test]
    fn the_section_shows_level_commands_and_grants_and_queues_actions() {
        let root = tempfile::tempdir().unwrap();
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        live.publish_command_packs(PackListing {
            rows: vec![PackRow {
                id: "demo-pack".into(),
                level: Level::Conformant,
                status: "Active".into(),
                commands: vec!["Demo: fit the view (demo-pack:fit)".into()],
                grants: "Curation".into(),
                last_failure: Some("not admitted: bad".into()),
            }],
            available: vec![root.path().join("other")],
        });
        pane.sync(640.0, 780.0);
        let button = |name: &str| taproot::Selector::role("button").with_attr("aria-label", name);
        for name in ["Unload demo-pack", "Install other", "Install other in dev mode"] {
            let found = taproot::matching_estimated_fixture(&*pane.dom_ref(), &button(name));
            assert!(!found.is_empty(), "{name}");
        }
        let reload = taproot::matching_estimated_fixture(&*pane.dom_ref(), &button("Reload demo-pack"));
        let _: Vec<()> = pane
            .runner
            .dispatch_click(reload[0], cambium::PointerClick::at((0.0, 0.0)));
        let level = taproot::matching_estimated_fixture(&*pane.dom_ref(), &button("Set demo-pack admission level"));
        let _: Vec<()> = pane
            .runner
            .dispatch_click(level[0], cambium::PointerClick::at((0.0, 0.0)));
        assert_eq!(
            live.take_actions(),
            [
                Action::ReloadCommandPack { id: "demo-pack".into() },
                Action::SetCommandPackLevel { id: "demo-pack".into(), dev: true },
            ]
        );
    }
}
