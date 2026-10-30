// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn live_appearance_changes_chrome_and_settings_paint_styles() {
    let appearance = crate::shell_services::AppearanceConfig {
        theme_id: Some("theme:dark".into()),
        theme_mode: crate::shell_services::ThemeMode::Light,
        ui_zoom: 1.5,
        ..Default::default()
    };
    let mut appearance = appearance;
    let resolved = tabard::resolve_theme_choice(
        &Default::default(),
        &tabard::theme::choice::ThemeChoice::new(
            "theme:dark",
            Some(tabard::theme::registry::Mode::Light),
        ),
    )
    .unwrap();
    appearance.theme_presentation = Some(resolved.presentation.clone());
    let chrome = chrome_sheet(&appearance);
    let cambium = cambium_sheet(&appearance);

    assert!(chrome.contains("--tabard-color-primary"));
    let tabard::ThemePresentation::Derived(palette) = &resolved.presentation else {
        unreachable!()
    };
    assert!(chrome.contains(&palette.css_custom_properties()));
    assert!(chrome.contains("24.00px"), "16px chrome input at 1.5x");
    assert!(cambium.contains("var(--tabard-color-bg)"));
    assert!(
        !cambium.contains("--tabard-color-bg,") && !chrome.contains("--tabard-color-text,"),
        "role rules read tabard's properties with no second fallback palette"
    );
    assert!(cambium.contains("19.50px"), "13px settings text at 1.5x");
    assert!(
        CAMBIUM_SHEET.contains(".radio { color: rgb(220, 226, 238)"),
        "the shared dark sheet gives controls an explicit readable foreground"
    );
}

#[test]
fn shared_canonical_roles_and_authored_rules_reach_the_retained_chrome_cascade() {
    let mut dom = ScriptedDom::new();
    let document = dom.document();
    dom.set_inner_html(
        document,
        "<html><body><div class='omni'><div class='omni-input'>Address</div></div></body></html>",
    );
    let omni = taproot::matching_estimated_fixture(&dom, &taproot::Selector::class("omni"))[0];
    let mut retained = RetainedLayout::new();
    let registry = tabard::theme::registry::ThemeRegistry::default();
    let mut observed = Vec::new();
    for mode in [
        tabard::theme::registry::Mode::Light,
        tabard::theme::registry::Mode::Dark,
        tabard::theme::registry::Mode::HcLight,
        tabard::theme::registry::Mode::HcDark,
    ] {
        let resolved = tabard::resolve_theme_choice(
            &registry,
            &tabard::theme::choice::ThemeChoice::new("theme:default", Some(mode)),
        )
        .unwrap();
        let tabard::ThemePresentation::Derived(palette) = &resolved.presentation else {
            panic!("canonical palette")
        };
        let expected = format!(
            "rgb({}, {}, {})",
            palette.palette.bg.r, palette.palette.bg.g, palette.palette.bg.b
        );
        let appearance = crate::shell_services::AppearanceConfig {
            theme_presentation: Some(resolved.presentation),
            ..Default::default()
        };
        let _ = retained.scene(&mut dom, &chrome_sheet(&appearance), 800, 600);
        let actual = retained
            .snapshot
            .as_ref()
            .unwrap()
            .styles
            .computed_style(omni, "background-color")
            .unwrap();
        assert_eq!(actual.replace(' ', ""), expected.replace(' ', ""));
        observed.push(actual);
    }
    for (index, color) in observed.iter().enumerate() {
        assert!(
            !observed[..index].contains(color),
            "four canonical modes have distinct actual backgrounds"
        );
    }
    let appearance = crate::shell_services::AppearanceConfig {
            theme_presentation: Some(tabard::ThemePresentation::AuthoredStylesheet(vec![":root { --tabard-color-bg: rgb(17, 34, 51); } .omni { border-color: rgb(61, 71, 81); }".into()])), ..Default::default()
        };
    let _ = retained.scene(&mut dom, &chrome_sheet(&appearance), 800, 600);
    let styles = &retained.snapshot.as_ref().unwrap().styles;
    assert_eq!(
        styles
            .computed_style(omni, "background-color")
            .unwrap()
            .replace(' ', ""),
        "rgb(17,34,51)"
    );
    assert_eq!(
        styles
            .computed_style(omni, "border-top-color")
            .unwrap()
            .replace(' ', ""),
        "rgb(61,71,81)"
    );
}

/// Canary for the chrome layer's load-bearing retained-layout behavior:
/// SIBLING absolutely-positioned subtrees must all emit paint. Caught
/// 2026-07-11: the retired genet-layout route only cascaded/boxed the FIRST element child
/// of a multi-root document (a host-built DOM with no `<html>` wrapper),
/// so the omnibar card blanked whenever the caption chip preceded it.
/// Fixed genet-side (multi-root box tree + cascade + root-background
/// gate); this stays as turnstone's tripwire on the behavior it leans on.
#[test]
fn chrome_absolute_siblings_all_paint() {
    let count = |style: &str, nested: bool| {
        let mut dom = ScriptedDom::new();
        let root = dom.document();
        let el = dom.create_element(qual("div"));
        dom.set_attribute(el, qual("class"), "omni");
        dom.set_attribute(el, qual("style"), style);
        if nested {
            let inner = dom.create_element(qual("div"));
            dom.set_attribute(inner, qual("class"), "omni-input");
            let t = dom.create_text("hello");
            dom.append_child(inner, t);
            dom.append_child(el, inner);
        } else {
            let t = dom.create_text("hello");
            dom.append_child(el, t);
        }
        dom.append_child(root, el);
        livery_paint_list(&dom, CHROME_SHEET, 1024, 600)
            .commands()
            .len()
    };
    let two_absolutes = {
        let mut dom = ScriptedDom::new();
        let root = dom.document();
        let chip = dom.create_element(qual("div"));
        dom.set_attribute(chip, qual("class"), "whereami");
        dom.set_attribute(chip, qual("style"), "transform: translate(12px, 566px);");
        let t = dom.create_text("alpha");
        dom.append_child(chip, t);
        dom.append_child(root, chip);
        let card = dom.create_element(qual("div"));
        dom.set_attribute(card, qual("class"), "omni");
        dom.set_attribute(
            card,
            qual("style"),
            "transform: translate(232px, 96px); width: 560px;",
        );
        let inner = dom.create_element(qual("div"));
        dom.set_attribute(inner, qual("class"), "omni-input");
        let t2 = dom.create_text("hello");
        dom.append_child(inner, t2);
        dom.append_child(card, inner);
        dom.append_child(root, card);
        livery_paint_list(&dom, CHROME_SHEET, 1024, 600)
            .commands()
            .len()
    };
    let chip_alone = count("transform: translate(232px, 96px); width: 560px;", false);
    let card_alone = count("transform: translate(232px, 96px); width: 560px;", true);
    assert!(
        two_absolutes > card_alone.max(chip_alone),
        "two absolute siblings must paint more than either alone \
             (chip={chip_alone} card={card_alone} together={two_absolutes}); \
             the second sibling's subtree is being dropped"
    );
}

#[test]
fn caret_editing_is_char_boundary_safe() {
    use crate::action::CaretMove;
    let mut s = OmnibarState {
        open: true,
        ..Default::default()
    };
    s.insert_str("mère");
    assert_eq!(s.cursor, s.text.len());
    s.move_caret(CaretMove::Left);
    s.move_caret(CaretMove::Left);
    s.move_caret(CaretMove::Left);
    assert!(s.backspace(), "removes the char before the caret");
    assert_eq!(s.text, "ère");
    assert!(s.delete_forward(), "removes the multibyte char after");
    assert_eq!(s.text, "re");
    s.move_caret(CaretMove::End);
    s.insert_str("x");
    s.move_caret(CaretMove::Home);
    s.insert_str("t");
    assert_eq!(s.text, "trex");
    s.move_caret(CaretMove::Home);
    assert!(!s.backspace(), "backspace at the line start is a no-op");
    s.move_caret(CaretMove::End);
    assert!(!s.delete_forward(), "delete at the line end is a no-op");
}

#[test]
fn address_normalization_recognizes_hosts_and_schemes() {
    assert_eq!(
        normalize_address("example.com").as_deref(),
        Some("https://example.com")
    );
    assert_eq!(
        normalize_address("gemini://x.y").as_deref(),
        Some("gemini://x.y")
    );
    assert_eq!(normalize_address("meerkats are great"), None);
    assert_eq!(normalize_address("nodots"), None);
}

#[test]
fn empty_and_command_texts_suggest_hints() {
    let canvas = Canvas::new();
    let mut state = OmnibarState {
        open: true,
        ..Default::default()
    };
    recompute_suggestions(&mut state, &canvas, &[]);
    assert!(matches!(state.suggestions[0], Suggestion::Hint(_)));
    state.text = ">set".into();
    recompute_suggestions(&mut state, &canvas, &[]);
    assert!(matches!(state.suggestions[0], Suggestion::Hint(_)));
}

#[test]
fn changed_certificate_prompt_exposes_both_complete_fingerprints() {
    let canvas = Canvas::new();
    let pinned = "11".repeat(32);
    let seen = "22".repeat(32);
    let mut state = OmnibarState {
        open: true,
        mode: OmnibarMode::GeminiTrust(GeminiTrustPrompt {
            node: Uuid::new_v4(),
            requested_url: "gemini://capsule.test/start".into(),
            fetch_url: "gemini://capsule.test/private".into(),
            target: "capsule.test".into(),
            pinned: pinned.clone(),
            seen: seen.clone(),
        }),
        ..Default::default()
    };

    recompute_suggestions(&mut state, &canvas, &[]);
    assert_eq!(state.suggestions.len(), 4);
    assert!(state.suggestions.iter().any(
        |row| matches!(row, Suggestion::Prompt(text) if text == &format!("Pinned: {pinned}"))
    ));
    assert!(state.suggestions.iter().any(
        |row| matches!(row, Suggestion::Prompt(text) if text == &format!("Presented: {seen}"))
    ));
}

/// The lane FILTERS the catalog it is handed, in the order it is handed
/// (the composition itself is `App::available_actions`). A caller passes the
/// static registry here; in the app it arrives with the contextual rows
/// already leading.
#[test]
fn actions_lane_filters_the_catalog_it_is_given() {
    let canvas = Canvas::new();
    let catalog: Vec<(String, crate::action::Action)> = crate::action::palette_actions();
    let mut state = OmnibarState {
        open: true,
        text: ">re".into(),
        ..Default::default()
    };
    recompute_suggestions(&mut state, &canvas, &catalog);
    assert!(
        state
            .suggestions
            .iter()
            .any(|s| matches!(s, Suggestion::Act { label, .. } if *label == "Reseed layout")),
        "`>re` must surface Reseed layout: {:?}",
        state.suggestions
    );
    assert!(
        !state
            .suggestions
            .iter()
            .any(|s| matches!(s, Suggestion::Act { label, .. } if *label == "Save session")),
        "`>re` must filter out non-matching actions"
    );
    // Bare `>` lists the catalog it was given, up to the row limit the
    // projection was asked for. The limit arrived with configurable chrome
    // (2026-08-10) and applies to every lane, so a catalog longer than the
    // limit is narrowed by typing rather than by scrolling a long list;
    // this entry point's limit is the legacy `MAX_NODE_MATCHES + 1`, while
    // the host passes `OmnibarConfig::row_limit`.
    state.text = ">".into();
    recompute_suggestions(&mut state, &canvas, &catalog);
    assert_eq!(
        state.suggestions.len(),
        catalog.len().min(MAX_NODE_MATCHES + 1),
        "the whole catalog, capped at the configured rows"
    );
}

#[test]
fn find_lane_matches_existing_nodes_before_go() {
    let mut canvas = Canvas::new();
    canvas.visit("https://example.com/meerkats");
    let mut state = OmnibarState {
        open: true,
        text: "example".into(),
        ..Default::default()
    };
    recompute_suggestions(&mut state, &canvas, &[]);
    assert!(
        matches!(&state.suggestions[0], Suggestion::Node { url, .. } if url.contains("example.com")),
        "an existing node outranks everything: {:?}",
        state.suggestions
    );
}
