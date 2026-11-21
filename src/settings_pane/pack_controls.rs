// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::engine_pack::{Capability, PackEdit, Scope};

pub(super) fn view(state: &SettingsState) -> SettingsView {
    let settings = state.live_settings.pack_settings();
    if settings.switches.is_empty() {
        return Box::new(el::<_, SettingsState, ()>("div", ()));
    }

    let profile = Keyed::new(
        settings
            .switches
            .iter()
            .map(|row| {
                let capability = row.capability;
                (
                    capability.label(),
                    switch_row(
                        capability,
                        "Profile default",
                        [
                            ("On", Scope::Profile(true), row.profile_enabled),
                            ("Off", Scope::Profile(false), !row.profile_enabled),
                        ],
                    ),
                )
            })
            .collect::<Vec<_>>(),
    );

    let session = settings.session.map(|generation| {
        let rows = Keyed::new(
            settings
                .switches
                .iter()
                .map(|row| {
                    let capability = row.capability;
                    let options = [
                        ("Use profile default", None),
                        ("On", Some(true)),
                        ("Off", Some(false)),
                    ]
                    .map(|(label, enabled)| {
                        (
                            label,
                            Scope::Session {
                                generation,
                                enabled,
                            },
                            row.session_override == enabled,
                        )
                    });
                    (
                        capability.label(),
                        switch_row(capability, "This session", options),
                    )
                })
                .collect::<Vec<_>>(),
        );
        (
            generation,
            el::<_, SettingsState, ()>(
                "div",
                (
                    el::<_, SettingsState, ()>("div", "This session")
                        .attr("class", "list-section-title"),
                    el::<_, SettingsState, ()>("div", rows),
                ),
            ),
        )
    });
    let notice = match (settings.session, settings.session_error) {
        (_, Some(error)) => format!("Could not load Smolweb session settings: {error}"),
        (None, None) => "Session overrides are unavailable.".into(),
        (Some(_), None) => String::new(),
    };
    Box::new(
        el::<_, SettingsState, ()>(
            "section",
            (
                el::<_, SettingsState, ()>("div", "Smolweb pack")
                    .attr("class", "list-section-title"),
                el::<_, SettingsState, ()>("div", "Profile default").attr("class", "setting-label"),
                el::<_, SettingsState, ()>("div", profile),
                el::<_, SettingsState, ()>("div", Keyed::new(session.into_iter().collect())),
                status_note::<SettingsState, ()>("list-row muted", notice),
            ),
        )
        .attr("aria-label", "Smolweb pack"),
    )
}

fn switch_row<const N: usize>(
    capability: Capability,
    scope_label: &'static str,
    options: [(&'static str, Scope, bool); N],
) -> SettingsView {
    let buttons = Keyed::new(
        options
            .into_iter()
            .map(|(label, scope, selected)| {
                (
                    label,
                    button(
                        label,
                        move |state: &mut SettingsState, _: cambium::PointerClick| {
                            state
                                .live_settings
                                .request_pack_edit(PackEdit { capability, scope });
                        },
                    )
                    .attr(
                        "aria-label",
                        format!("{}, {scope_label}, {label}", capability.label()),
                    )
                    .attr("aria-pressed", selected.to_string())
                    .attr("class", "setting-apply pack-switch"),
                )
            })
            .collect::<Vec<_>>(),
    );
    Box::new(
        el::<_, SettingsState, ()>(
            "div",
            (
                el::<_, SettingsState, ()>("div", capability.label())
                    .attr("class", "pack-switch-label"),
                buttons,
            ),
        )
        .attr("class", "pack-switch-row"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_settings_expose_and_dispatch_independent_accessible_choices() {
        let root = tempfile::tempdir().unwrap();
        let pack = crate::engine_pack::SmolwebPack::new(root.path());
        let live = LiveSettingsHandle::new(&ApplicationSettings::default());
        let mut pane = SettingsPane::with_live_settings(root.path().into(), Some(live.clone()));
        let selector = |name: &str| taproot::Selector::role("button").with_attr("aria-label", name);
        let session_markdown = selector("Markdown parser, This session, Off");
        assert!(
            taproot::matching_estimated_fixture(&*pane.dom_ref(), &session_markdown).is_empty()
        );

        pack.install_test_fixture();
        pack.bind_session(Some(root.path()));
        let settings = pack.settings();
        let generation = settings.session.unwrap();
        let markdown = settings
            .switches
            .iter()
            .find(|row| row.capability.label() == "Markdown parser")
            .unwrap()
            .capability;
        let spartan = settings
            .switches
            .iter()
            .find(|row| row.capability.label() == "Spartan protocol")
            .unwrap()
            .capability;
        live.publish_pack_settings(settings);
        pane.sync(640.0, 780.0);

        let node = taproot::matching_estimated_fixture(&*pane.dom_ref(), &session_markdown)[0];
        pane.runner.set_focus(Some(node));
        pane.key(cambium::KeyEvent::new(cambium::Key::Named(
            cambium::NamedKey::Enter,
        )));
        let profile_spartan = selector("Spartan protocol, Profile default, Off");
        let node = taproot::matching_estimated_fixture(&*pane.dom_ref(), &profile_spartan)[0];
        let _: Vec<()> = pane
            .runner
            .dispatch_click(node, cambium::PointerClick::at((0.0, 0.0)));
        let edits = live.take_pack_edits();
        assert_eq!(
            edits,
            [
                PackEdit {
                    capability: markdown,
                    scope: Scope::Session {
                        generation,
                        enabled: Some(false)
                    }
                },
                PackEdit {
                    capability: spartan,
                    scope: Scope::Profile(false)
                },
            ]
        );
        for edit in edits {
            pack.edit(edit).unwrap();
        }
        live.publish_pack_settings(pack.settings());
        pane.sync(640.0, 780.0);
        let pressed = |name| {
            !taproot::matching_estimated_fixture(
                &*pane.dom_ref(),
                &selector(name).with_attr("aria-pressed", "true"),
            )
            .is_empty()
        };
        assert!(pressed("Markdown parser, This session, Off"));
        assert!(pressed("Spartan protocol, Profile default, Off"));
        assert!(pressed(
            "Spartan protocol, This session, Use profile default"
        ));
        assert!(pressed("Markdown parser, Profile default, On"));
    }
}
