// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Recompute suggestions while respecting the shell's configured visible-row
/// limit. The legacy entry point above preserves its existing six node rows
/// plus one literal-address row behavior for callers outside the host.
pub fn recompute_suggestions_with_limit(
    state: &mut OmnibarState,
    canvas: &Canvas,
    actions: &[(String, crate::action::Action)],
    recall: &[crate::action::RecallHit],
    row_limit: usize,
) {
    state.suggestions.clear();

    if let OmnibarMode::SmolwebInput(input) = &state.mode {
        state
            .suggestions
            .push(Suggestion::Prompt(if input.sensitive {
                format!(
                    "Sensitive input \u{00b7} {} \u{00b7} Enter to submit",
                    input.prompt
                )
            } else {
                format!("{} \u{00b7} Enter to submit", input.prompt)
            }));
        state.selected = 0;
        return;
    }

    if let OmnibarMode::SmolwebSubmission(submission) = &state.mode {
        let prompt = match submission.stage {
            SmolwebSubmissionStage::Body => match &submission.file_name {
                Some(name) => format!("Body from {name} · Enter to continue or drop another file"),
                None => "Type the submission body, or drop a file · Enter to continue".to_string(),
            },
            SmolwebSubmissionStage::Mime => {
                "MIME type · edit the suggested value · Enter to continue".to_string()
            },
            SmolwebSubmissionStage::Token => {
                "Optional Titan token · input is hidden · Enter to continue".to_string()
            },
            SmolwebSubmissionStage::Confirm => format!(
                "Send {} bytes to {} · type send and press Enter",
                submission.body.len(),
                submission.target
            ),
        };
        state.suggestions.push(Suggestion::Prompt(prompt));
        state.selected = 0;
        return;
    }

    if let OmnibarMode::MicronForm(prompt) = &state.mode {
        let message = match prompt.stage {
            MicronFormStage::Action => {
                let labels = prompt
                    .form
                    .actions()
                    .iter()
                    .enumerate()
                    .map(|(index, action)| format!("{}: {}", index + 1, action.label))
                    .collect::<Vec<_>>()
                    .join(" · ");
                format!("Choose submit action ({labels})")
            },
            MicronFormStage::Field(index) => match prompt.form.fields().get(index) {
                Some(field) if field.masked() => {
                    format!("{} · input is hidden · Enter to continue", field.name())
                },
                Some(field) => match &field.kind {
                    nematic::micron::syntax::FieldKind::Checkbox { .. } => {
                        format!(
                            "{} = {} · use on or off · Enter to continue",
                            field.name(),
                            field.value()
                        )
                    },
                    nematic::micron::syntax::FieldKind::Radio { .. } => {
                        format!(
                            "{} = {} · use on to select (clears its peer) or off · Enter to continue",
                            field.name(),
                            field.value()
                        )
                    },
                    nematic::micron::syntax::FieldKind::Text {
                        rows: Some(rows), ..
                    } if *rows > 1 => {
                        format!(
                            "{} · paste multiline text if needed · Enter to continue",
                            field.name()
                        )
                    },
                    _ => format!("{} · Enter to continue", field.name()),
                },
                None => "Micron form is invalid".to_string(),
            },
            MicronFormStage::Confirm => {
                let label = prompt
                    .form
                    .actions()
                    .get(prompt.action)
                    .map(|action| action.label.as_str())
                    .unwrap_or("submit");
                let target = prompt
                    .resolved_target
                    .as_deref()
                    .unwrap_or("invalid target");
                format!("{} · {target} · type send and press Enter", label)
            },
        };
        state.suggestions.push(Suggestion::Prompt(message));
        if let Some(error) = &prompt.error {
            state.suggestions.push(Suggestion::Prompt(error.clone()));
        }
        state.selected = 0;
        return;
    }

    if let OmnibarMode::SmolwebSubmissionResult(result) = &state.mode {
        state.suggestions.push(Suggestion::Prompt(format!(
            "{} · {} · Enter to close",
            result.target, result.message
        )));
        state.selected = 0;
        return;
    }

    if let OmnibarMode::GeminiIdentity(input) = &state.mode {
        let origin = crate::gemini_identity::capsule_origin(&input.identity_url)
            .unwrap_or_else(|_| "this Gemini capsule".to_string());
        state.suggestions.push(Suggestion::Prompt(format!(
            "Create a client identity for {origin} \u{00b7} {} \u{00b7} Enter to create",
            input.prompt
        )));
        state.selected = 0;
        return;
    }

    if let OmnibarMode::GeminiTrust(input) = &state.mode {
        state.suggestions.extend([
            Suggestion::Prompt(format!("Certificate changed for {}", input.target)),
            Suggestion::Prompt(format!("Pinned: {}", input.pinned)),
            Suggestion::Prompt(format!("Presented: {}", input.seen)),
            Suggestion::Prompt("Type trust and press Enter to replace the pin".to_string()),
        ]);
        state.selected = 0;
        return;
    }

    // Place prompts capture the whole line, exactly like rename.
    if let OmnibarMode::Place(prompt) = state.mode {
        state.suggestions.push(Suggestion::Hint(prompt.hint()));
        state.selected = 0;
        return;
    }

    // Rename mode captures the whole line as the new name; no lane matching.
    if matches!(state.mode, OmnibarMode::RenameSession(_)) {
        state
            .suggestions
            .push(Suggestion::Hint("Enter to rename this session"));
        state.selected = 0;
        return;
    }

    let text = state.text.trim();

    if let Some(rest) = text.strip_prefix('>') {
        // The actions lane FILTERS the catalog it is given, in the order it is
        // given. The composition (contextual rows leading the static registry)
        // belongs to `App::available_actions`, so the lane, the snapshot, and
        // the automation runner all offer and resolve the same list.
        let needle = rest.trim().to_lowercase();
        state.suggestions.extend(
            actions
                .iter()
                .cloned()
                .filter(|(label, _)| needle.is_empty() || label.to_lowercase().contains(&needle))
                .map(|(label, action)| Suggestion::Act { label, action }),
        );
        if state.suggestions.is_empty() {
            state
                .suggestions
                .push(Suggestion::Hint("no matching action"));
        }
        state.suggestions.truncate(row_limit.max(1));
        state.selected = state.selected.min(state.suggestions.len() - 1);
        return;
    }

    if text.is_empty() {
        state
            .suggestions
            .push(Suggestion::Hint("type an address, or > for actions"));
        state.selected = 0;
        return;
    }

    // Find: match existing nodes on label / host / url, newest visit first.
    let needle = text.to_lowercase();
    let graph = canvas.graph();
    let mut matches: Vec<(std::time::SystemTime, Suggestion)> = graph
        .nodes()
        .filter_map(|(key, node)| {
            let label = graph.node_display_label(key);
            let host = url::Url::parse(node.url())
                .ok()
                .and_then(|parsed| parsed.host_str().map(str::to_owned))
                .unwrap_or_default();
            let url = node.url().to_string();
            let hit = label.to_lowercase().contains(&needle)
                || host.to_lowercase().contains(&needle)
                || url.to_lowercase().contains(&needle);
            hit.then(|| {
                (
                    graph
                        .node_last_visited(key)
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                    Suggestion::Node {
                        id: node.id,
                        url,
                        label,
                        host,
                    },
                )
            })
        })
        .collect();
    matches.sort_by(|a, b| b.0.cmp(&a.0));
    state
        .suggestions
        .extend(matches.into_iter().take(MAX_NODE_MATCHES).map(|(_, s)| s));

    // Go: address-shaped input opens (mint-or-select + fetch).
    if let Some(url) = normalize_address(text) {
        state.suggestions.push(Suggestion::Go { url });
    }

    // Recall: pages this persona visited before, from browsing memory. They
    // land AFTER the go row deliberately — a full address typed into the line
    // must keep committing to itself on Enter, whatever the trail remembers.
    // A page still in the graph already showed as a Node row above, so the
    // lane skips it rather than saying the same page twice.
    let shown: Vec<String> = state
        .suggestions
        .iter()
        .filter_map(|s| match s {
            Suggestion::Node { url, .. } | Suggestion::Go { url } => Some(url.clone()),
            _ => None,
        })
        .collect();
    let recall_rows: Vec<Suggestion> = recall
        .iter()
        .filter(|hit| !shown.contains(&hit.url))
        .map(|hit| Suggestion::Recall {
            url: hit.url.clone(),
            title: hit.title.clone(),
            at_ms: hit.at_ms,
        })
        .collect();
    if !recall_rows.is_empty() {
        // The lane needs a footing inside the configured budget, which is
        // sized for six node matches plus the go row — without one, a busy
        // graph would silently swallow every recalled page. So recall is
        // guaranteed a small share by displacing the LEAST-RECENT node
        // matches, never by growing the list past the row limit the setting
        // asked for. The go row is never displaced: it is what Enter commits.
        let go_row = matches!(state.suggestions.last(), Some(Suggestion::Go { .. }))
            .then(|| state.suggestions.pop())
            .flatten();
        let footing = recall_rows.len().min((row_limit / 3).max(1));
        let node_budget = row_limit
            .saturating_sub(footing + usize::from(go_row.is_some()))
            .max(1);
        state.suggestions.truncate(node_budget);
        state.suggestions.extend(go_row);
        state.suggestions.extend(recall_rows);
    }

    if state.suggestions.is_empty() {
        state
            .suggestions
            .push(Suggestion::Hint("no matching nodes; enter an address"));
    }
    state.suggestions.truncate(row_limit.max(1));
    state.selected = state.selected.min(state.suggestions.len() - 1);
}
