// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn parse_errors_name_their_line() {
    let err = parse(
        "open mere://x
frobnicate
",
    )
    .unwrap_err();
    assert!(err.contains("line 2"), "{err}");
}

#[test]
fn pointer_move_is_a_distinct_scenario_step() {
    let steps = parse("move-at 310 303").unwrap();
    let [Step::Move(x, y)] = steps.as_slice() else {
        panic!("move-at did not parse as a pointer move");
    };
    assert_eq!((*x, *y), (310.0, 303.0));
}

#[test]
fn browser_receipt_assertions_are_typed_steps() {
    let steps = parse("assert fetch stopped\nassert link-preview /inside").unwrap();
    assert!(matches!(
        steps.as_slice(),
        [Step::AssertBrowserFetch(fetch), Step::AssertLinkPreview(link)]
            if fetch == "stopped" && link == "/inside"
    ));
}

#[test]
fn file_hover_and_drop_are_distinct_ui_turns() {
    let steps = parse(
        "hover-file 350 280 receipt.txt
drop-file 350 280 receipt.txt",
    )
    .unwrap();
    assert!(matches!(steps[0], Step::HoverFile(350.0, 280.0, ref path) if path == "receipt.txt"));
    assert!(matches!(steps[1], Step::DropFile(350.0, 280.0, ref path) if path == "receipt.txt"));
}

#[test]
fn save_chord_is_one_focus_routed_key_step() {
    let steps = parse("key ctrl+s").unwrap();
    assert!(matches!(steps.as_slice(), [Step::Key(EditKey::Save)]));
}

#[test]
fn find_chord_is_one_focus_routed_key_step() {
    let steps = parse("key ctrl+f").unwrap();
    assert!(matches!(steps.as_slice(), [Step::Key(EditKey::Find)]));
}

#[test]
fn tab_is_a_focus_routed_key_step() {
    let steps = parse("key tab").unwrap();
    assert!(matches!(steps.as_slice(), [Step::Key(EditKey::Tab)]));
}

#[test]
fn wait_row_wait_status_wait_file_are_typed_steps() {
    let steps = parse(
            "wait-row 300 hello\nwait-status 60 Local rendezvous\nwait-file 3000 C:\\t\\exchange\\demo.place-card.json",
        )
        .unwrap();
    assert!(matches!(
        steps.as_slice(),
        [Step::WaitRow(300, row), Step::WaitStatus(60, status), Step::WaitFile(3000, path)]
            if row == "hello"
            && status == "Local rendezvous"
            && path == "C:\\t\\exchange\\demo.place-card.json"
    ));
}

#[test]
fn physical_ime_scenarios_use_valid_native_harness_grammar() {
    for source in [
        include_str!("../../scenarios/cue_ime_linux.scn"),
        include_str!("../../scenarios/cue_ime_sky_linux.scn"),
    ] {
        let source = source
            .replace("${CUE_IME_EXCHANGE}", "/tmp/cue-ime-exchange")
            .replace("${SERVO_FIXTURE_BASE}", "http://127.0.0.1:12345");
        taproot::Scenario::parse(&source).expect("valid native harness grammar");
    }
}

#[test]
fn wait_knot_is_a_typed_step() {
    let steps = parse("wait-knot 600 visiting knot://abcd/field.knot").unwrap();
    assert!(matches!(
        steps.as_slice(),
        [Step::WaitKnot(600, substr)] if substr == "visiting knot://abcd/field.knot"
    ));
}

#[test]
fn record_place_is_a_typed_step() {
    let steps = parse("record-place founder").unwrap();
    assert!(matches!(steps.as_slice(), [Step::RecordPlace(name)] if name == "founder"));
}

#[test]
fn saved_source_observation_commands_parse_and_reject_bad_arguments() {
    let steps = parse("wait-saved-sources 600\nwait-saved-reader 300\nrecord-saved-sources sample")
        .unwrap();
    assert!(matches!(
        steps.as_slice(),
        [
            Step::WaitSavedSources(600),
            Step::WaitSavedReader(300),
            Step::RecordSavedSources(name),
        ] if name == "sample"
    ));

    for invalid in [
        "wait-saved-sources",
        "wait-saved-sources many",
        "wait-saved-sources 10 extra",
        "wait-saved-reader",
        "wait-saved-reader -1",
        "wait-saved-reader 10 extra",
        "record-saved-sources",
    ] {
        assert!(
            parse(invalid).is_err(),
            "accepted invalid command: {invalid}"
        );
    }
}

#[test]
fn touch_is_a_typed_step() {
    let steps = parse("touch C:\\t\\exchange\\joiner.stopped").unwrap();
    assert!(matches!(
        steps.as_slice(),
        [Step::TouchFile(path)] if path == "C:\\t\\exchange\\joiner.stopped"
    ));
}

#[test]
fn env_substitution_expands_at_parse_time_backslashes_and_colons_intact() {
    // SAFETY: test-local env var, no other test in this process reads it.
    unsafe { std::env::set_var("TURNSTONE_TEST_EXCHANGE", "C:\\t\\exchange") };
    let steps = parse("type ${TURNSTONE_TEST_EXCHANGE}\\demo.place-card.json").unwrap();
    unsafe { std::env::remove_var("TURNSTONE_TEST_EXCHANGE") };
    assert!(matches!(
        steps.as_slice(),
        [Step::Type(text)] if text == "C:\\t\\exchange\\demo.place-card.json"
    ));
}

#[test]
fn env_substitution_names_the_missing_variable_and_line() {
    let err = parse("wait-file 60 ${TURNSTONE_TEST_DOES_NOT_EXIST}\\card.json").unwrap_err();
    assert!(err.contains("line 1"), "{err}");
    assert!(err.contains("TURNSTONE_TEST_DOES_NOT_EXIST"), "{err}");
}

#[test]
fn type_preserves_backslashes_and_colons_unchanged() {
    let steps = parse("type C:\\t\\exchange\\demo.place-card.json").unwrap();
    assert!(matches!(
        steps.as_slice(),
        [Step::Type(text)] if text == "C:\\t\\exchange\\demo.place-card.json"
    ));
}

#[test]
fn idle_diagnostics_and_document_idle_are_typed_steps() {
    let steps =
        parse("record-idle after-fetch\nassert content-sessions-idle\nassert content-ready")
            .unwrap();
    assert!(matches!(
        steps.as_slice(),
        [Step::RecordIdle(name), Step::AssertContentSessionsIdle, Step::AssertContentReady] if name == "after-fetch"
    ));
}
#[test]
fn inline_component_station_fixture_uses_supported_grammar() {
    taproot::Scenario::parse(include_str!("../../scenarios/a4_inline_components.scn")).unwrap();
    taproot::Scenario::parse(include_str!("../../scenarios/a4_native_overlap.scn")).unwrap();
    taproot::Scenario::parse(include_str!("../../scenarios/frame_lens_scroll.scn")).unwrap();
    taproot::Scenario::parse(include_str!("../../scenarios/frame_appearance.scn")).unwrap();
    taproot::Scenario::parse(include_str!("../../scenarios/frame_appearance_reopen.scn")).unwrap();
    taproot::Scenario::parse(include_str!("../../scenarios/frame_fonts_workshop.scn")).unwrap();
    taproot::Scenario::parse(include_str!(
        "../../scenarios/frame_fonts_missing_workshop.scn"
    ))
    .unwrap();
    assert!(matches!(
        parse("captures-differ before after\nscroll-pane-window 1 1400 400 0 300")
            .unwrap()
            .as_slice(),
        [Step::CapturesDiffer(before, after), Step::ScrollPaneWindow(1, ..)]
            if before == "before" && after == "after"
    ));
    assert!(matches!(
        parse("key-window 1 ctrl+f6").unwrap().as_slice(),
        [Step::KeyWindow(1, EditKey::FrameFocus)]
    ));
    assert!(matches!(
        parse("click-content-window 1 600 250").unwrap().as_slice(),
        [Step::ClickContentWindow(1, x, y)] if *x == 600.0 && *y == 250.0
    ));
    for invalid in [
        "click-content-window 1 NaN 2",
        "click-content-window -1 2 3",
        "click-content-window 1 2 3 extra",
        "click-content-window 1 2",
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
    crate::frame_a11y::fixture_body_request("1 click tab Links").unwrap();
    assert!(crate::frame_a11y::fixture_body_request("1 replace tab Links").is_err());
}

#[test]
fn model_free_structured_observation_commands_parse_safely() {
    let steps = parse("record-structured-research alpha_1\nchoose-suggestion Count by source kind")
        .unwrap();
    assert!(matches!(steps.as_slice(),
        [Step::RecordStructuredResearch(name), Step::ChooseSuggestion(label)]
        if name == "alpha_1" && label == "Count by source kind"));
    for input in [
        "record-structured-research",
        "record-structured-research ../x",
        "record-structured-research a/b",
        "choose-suggestion",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn model_free_suggestion_selection_refuses_ambiguity_and_prefers_exact() {
    let labels = vec![
        "› Count by source kind".into(),
        "› Count by source".into(),
        "› Back".into(),
    ];
    assert_eq!(suggestion_target(&labels, "Count by source").unwrap(), 1);
    assert!(suggestion_target(&labels, "Count by").is_err());
    assert!(suggestion_target(&labels, "absent").is_err());
    let duplicates = vec!["› Back".into(), "› Back".into()];
    assert!(suggestion_target(&duplicates, "Back").is_err());
}

#[test]
fn model_free_status_observation_rejects_unsafe_names() {
    assert!(
        matches!(parse("record-structured-status status_1").unwrap().as_slice(),
        [Step::RecordStructuredStatus(name)] if name == "status_1")
    );
    assert!(parse("record-structured-status ../status").is_err());
}

#[test]
fn model_free_runtime_observation_name_is_bounded_to_one_file() {
    assert!(matches!(parse("record-runtime cold_1").unwrap().as_slice(),
        [Step::RecordRuntime(name)] if name == "cold_1"));
    assert!(parse("record-runtime ../outside").is_err());
}

#[test]
fn model_free_capture_choice_requires_exact_current_session_and_ordinal() {
    let value = serde_json::json!([
        {"session":"s", "ordinal":1, "phase":"saved", "stored":{"annotation_manifest":"blake3:a"}},
        {"session":"s", "ordinal":2, "phase":"saved", "stored":{"annotation_manifest":"blake3:b"}},
        {"session":"foreign", "ordinal":1, "phase":"saved", "stored":{"annotation_manifest":"blake3:c"}}
    ]);
    assert_eq!(
        observed_capture_manifest(&value, "s", 1).unwrap(),
        "blake3:a"
    );
    assert_eq!(
        observed_capture_manifest(&value, "s", 2).unwrap(),
        "blake3:b"
    );
    assert!(observed_capture_manifest(&value, "s", 3).is_err());
    assert!(parse("choose-captured-source ../outside 1").is_err());
    assert!(parse("choose-captured-source captured 0").is_err());
    assert!(
        matches!(parse("choose-captured-source captured 1").unwrap().as_slice(),
        [Step::ChooseCapturedSource { name, ordinal: 1 }] if name == "captured")
    );
}

#[test]
fn model_free_comparison_observation_name_is_safe() {
    assert!(
        matches!(parse("record-comparison exact_pair").unwrap().as_slice(),
        [Step::RecordComparison(name)] if name == "exact_pair")
    );
    assert!(parse("record-comparison ../pair").is_err());
}

#[test]
fn model_free_saved_result_choice_requires_saved_current_identity() {
    let mut value = serde_json::json!({"schema":"turnstone.structured-observation/v1",
        "session":"s", "historical":false, "busy":false,
        "saved_manifest":"blake3:a", "content_manifest":"blake3:a"});
    assert_eq!(observed_saved_result(&value, "s").unwrap(), "blake3:a");
    assert!(observed_saved_result(&value, "foreign").is_err());
    value["content_manifest"] = serde_json::json!("blake3:b");
    assert!(observed_saved_result(&value, "s").is_err());
    assert!(parse("choose-saved-result ../outside").is_err());
    assert!(
        matches!(parse("choose-saved-result seed_sum").unwrap().as_slice(),
        [Step::ChooseSavedResult(name)] if name == "seed_sum")
    );
}

#[test]
fn model_free_note_pointer_and_private_action_commands_parse() {
    assert!(matches!(
        parse("click-note-member").unwrap().as_slice(),
        [Step::ClickNoteMember]
    ));
    assert!(parse("click-note-member another").is_err());
    assert!(
        matches!(parse("assert-no-action Retain selected Redshank note").unwrap().as_slice(),
        [Step::AssertNoAction(label)] if label == "Retain selected Redshank note")
    );
    assert!(parse("assert-no-action").is_err());
}

#[test]
fn model_free_search_wait_requires_a_frame_limit() {
    assert!(matches!(
        parse("wait-research-search 600").unwrap().as_slice(),
        [Step::WaitResearchSearch(600)]
    ));
    assert!(parse("wait-research-search invalid").is_err());
}

#[test]
fn model_free_structured_result_wait_requires_a_frame_limit() {
    assert!(matches!(
        parse("wait-structured-result 600").unwrap().as_slice(),
        [Step::WaitStructuredResult(600)]
    ));
    assert!(parse("wait-structured-result invalid").is_err());
    assert!(matches!(
        parse("wait-structured-author-save 600").unwrap().as_slice(),
        [Step::WaitStructuredAuthorSave(600)]
    ));
    assert!(parse("wait-structured-author-save invalid").is_err());
    assert!(matches!(parse("wait-structured-idle 600").unwrap().as_slice(), [Step::WaitStructuredIdle(600)]));
    assert!(matches!(parse("wait-structured-limit-refusal 600").unwrap().as_slice(), [Step::WaitStructuredLimitRefusal(600)]));
    assert!(parse("wait-structured-idle invalid").is_err());
    assert!(parse("wait-structured-limit-refusal invalid").is_err());
}

#[test]
fn model_free_source_target_wait_requires_frames_and_exact_url() {
    assert!(
        matches!(parse("wait-source-target 600 https://example.test/A").unwrap().as_slice(),
        [Step::WaitSourceTarget(600, url)] if url == "https://example.test/A")
    );
    assert!(parse("wait-source-target 600").is_err());
    assert!(parse("wait-source-target invalid https://example.test/A").is_err());
}
