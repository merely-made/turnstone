// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn context(pane: u64) -> ContextSnapshot {
    ContextSnapshot {
        pane: Some(PaneId(pane)),
        ..ContextSnapshot::default()
    }
}

#[test]
fn transcript_binds_the_target_at_open_and_replay_keeps_it() {
    let mut services = ShellServices::default();
    services.begin_omnibar(context(7));
    let original = services.record_omnibar(
        ShellInput::Omnibar("mere://field-notes".into()),
        ShellIntent::Navigate {
            url: "mere://field-notes".into(),
        },
        context(99),
        EntryPrivacy::Ordinary,
    );
    services.complete(
        original,
        ShellOutcome::Completed {
            summary: "opened mere://field-notes".into(),
        },
    );

    let (repeat, replay) = services.repeat(original).expect("entry replays");
    assert_eq!(replay.target, context(7));
    assert_eq!(
        services.transcript().entry(repeat).unwrap().target,
        context(7)
    );
    assert!(matches!(
        services.transcript().entry(repeat).unwrap().outcome,
        ShellOutcome::Pending
    ));
}

#[test]
fn retention_and_redaction_bound_the_local_ledger() {
    let mut transcript = ShellTranscript::default();
    transcript.set_retention(TranscriptRetention { max_entries: 2 });
    for input in ["one", "two", "three"] {
        transcript.record(
            ShellInput::Omnibar(input.into()),
            ShellIntent::Navigate { url: input.into() },
            ContextSnapshot::default(),
            EntryPrivacy::Redacted,
        );
    }
    let entries: Vec<_> = transcript.entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, ShellEntryId(2));
    assert!(matches!(entries[0].input, ShellInput::Redacted { .. }));
    assert_eq!(entries[0].privacy, EntryPrivacy::Redacted);
}

#[test]
fn blueprint_composition_changes_only_projection_choices() {
    let mut chrome = ShellChromeConfig::default();
    let blueprint = ChromeBlueprint {
        omnibar: ChromePlacement::Docked(ChromeEdge::Bottom),
        shellbar: ChromePlacement::Hidden,
        transcript: ChromePlacement::Floating,
        status: ChromePlacement::Docked(ChromeEdge::Bottom),
    };
    chrome.apply_blueprint(&blueprint);
    assert_eq!(chrome.omnibar.placement, blueprint.omnibar);
    assert!(!chrome.shellbar.visible);
    assert_eq!(chrome.transcript_placement, ChromePlacement::Floating);
    // Against the default rather than a literal: the invariant is that a
    // blueprint moves placements and touches no policy value, whatever
    // that value currently is.
    assert_eq!(
        chrome.omnibar.row_limit,
        OmnibarConfig::default().row_limit,
        "layout does not own policy"
    );
}

#[test]
fn default_providers_and_shortcuts_are_registered_without_a_canvas() {
    let mut services = ShellServices::default();
    let commands = ShellProviderId::new("turnstone.actions");
    assert!(services.providers().get(&commands).is_some());
    assert_eq!(
        services.shortcut_scope("K", true),
        Some(OmnibarScope::Command)
    );
    services.providers_mut().unregister(&commands);
    assert!(services.providers().get(&commands).is_none());
}

#[test]
fn system_follows_the_os_preference_and_chosen_modes_do_not() {
    use tabard::theme::registry::Mode;
    let mut appearance = AppearanceConfig::default();
    assert_eq!(appearance.tabard_mode(), Mode::Dark, "dark until observed");
    appearance.system_dark = false;
    assert_eq!(appearance.tabard_mode(), Mode::Light);
    appearance.theme_mode = ThemeMode::Dark;
    assert_eq!(appearance.tabard_mode(), Mode::Dark);
    appearance.theme_mode = ThemeMode::HcDark;
    assert_eq!(appearance.tabard_mode(), Mode::HcDark);
}
