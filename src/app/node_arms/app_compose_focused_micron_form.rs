// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl App {
    pub(in super::super) fn compose_focused_micron_form(&mut self) -> Vec<Effect> {
        let Some(source) = self.graph_runtimes.focused_member() else {
            return vec![Effect::Redraw];
        };
        let Some(url) = self
            .graph_runtimes
            .graph()
            .get_node_by_id(source)
            .map(|(_, node)| node.url().to_owned())
        else {
            return vec![Effect::Redraw];
        };
        if !crate::nomadnet::is_micron_address(&url) {
            self.events.push(AppEvent::SmolwebSubmissionFailed {
                target: url,
                error: "the focused page is not Micron".to_string(),
            });
            return vec![Effect::Redraw];
        }
        let Some(document) = self.content.fetched(source, &url).cloned() else {
            self.events.push(AppEvent::SmolwebSubmissionFailed {
                target: url,
                error: "the focused Micron page is not available for form editing".to_string(),
            });
            return vec![Effect::Redraw];
        };
        let form = match nematic::micron::forms::FormState::from_source(
            &document.body,
            nematic::micron::forms::FormLimits::default(),
        ) {
            Ok(form) if !form.actions().is_empty() => form,
            Ok(_) => {
                self.events.push(AppEvent::SmolwebSubmissionFailed {
                    target: url,
                    error: "the focused Micron page has no submit actions".to_string(),
                });
                return vec![Effect::Redraw];
            },
            Err(error) => {
                self.events
                    .push(AppEvent::SmolwebSubmissionFailed { target: url, error });
                return vec![Effect::Redraw];
            },
        };
        let initial_target = (form.actions().len() == 1)
            .then(|| self.resolve_micron_form_target(source, &form.actions()[0].target))
            .flatten();
        let target_context = self.fallback_shell_context();
        self.shell.begin_omnibar(target_context);
        let stage = if form.actions().len() == 1 && form.fields().is_empty() {
            crate::ui::MicronFormStage::Confirm
        } else if form.actions().len() == 1 {
            crate::ui::MicronFormStage::Field(0)
        } else {
            crate::ui::MicronFormStage::Action
        };
        let text = match stage {
            crate::ui::MicronFormStage::Field(0) => form
                .fields()
                .first()
                .map(micron_field_editor_value)
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.omnibar = crate::ui::OmnibarState {
            open: true,
            mode: crate::ui::OmnibarMode::MicronForm(crate::ui::MicronFormPrompt {
                source,
                source_url: url,
                source_body: document.body,
                form,
                action: 0,
                resolved_target: initial_target,
                stage,
                error: None,
            }),
            text,
            ..Default::default()
        };
        self.omnibar.cursor = self.omnibar.text.len();
        self.focus = FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    pub(super) fn commit_micron_form(
        &mut self,
        mut prompt: crate::ui::MicronFormPrompt,
    ) -> Vec<Effect> {
        use crate::ui::MicronFormStage;
        prompt.error = None;
        match prompt.stage {
            MicronFormStage::Action => {
                let Ok(choice) = self.omnibar.text.trim().parse::<usize>() else {
                    return self.micron_form_error(prompt, "Enter a submit action number".into());
                };
                let Some(action) = choice
                    .checked_sub(1)
                    .filter(|index| *index < prompt.form.actions().len())
                else {
                    return self.micron_form_error(prompt, "Unknown submit action number".into());
                };
                prompt.action = action;
                prompt.stage = if prompt.form.fields().is_empty() {
                    MicronFormStage::Confirm
                } else {
                    MicronFormStage::Field(0)
                };
                prompt.resolved_target = self.resolve_micron_form_target(
                    prompt.source,
                    &prompt.form.actions()[action].target,
                );
                self.omnibar.text = prompt
                    .form
                    .fields()
                    .first()
                    .map(micron_field_editor_value)
                    .unwrap_or_default();
            },
            MicronFormStage::Field(index) => {
                let Some(field) = prompt.form.fields().get(index) else {
                    return vec![Effect::Redraw];
                };
                let result =
                    if matches!(field.kind, nematic::micron::syntax::FieldKind::Text { .. }) {
                        prompt.form.set_text(index, self.omnibar.text.clone())
                    } else {
                        match self.omnibar.text.trim().to_ascii_lowercase().as_str() {
                            "on" | "yes" | "true" | "1" => prompt.form.set_checked(index, true),
                            "off" | "no" | "false" | "0" | "" => {
                                prompt.form.set_checked(index, false)
                            },
                            _ => {
                                return self.micron_form_error(
                                    prompt,
                                    "Enter on or off for this choice".into(),
                                );
                            },
                        }
                    };
                if let Err(error) = result {
                    return self.micron_form_error(prompt, error);
                }
                let next = index + 1;
                prompt.stage = if next < prompt.form.fields().len() {
                    MicronFormStage::Field(next)
                } else {
                    MicronFormStage::Confirm
                };
                if matches!(prompt.stage, MicronFormStage::Confirm) {
                    prompt.resolved_target = self.resolve_micron_form_target(
                        prompt.source,
                        &prompt.form.actions()[prompt.action].target,
                    );
                }
                self.omnibar.text = match prompt.stage {
                    MicronFormStage::Field(next) => prompt
                        .form
                        .fields()
                        .get(next)
                        .map(micron_field_editor_value)
                        .unwrap_or_default(),
                    MicronFormStage::Confirm => String::new(),
                    MicronFormStage::Action => unreachable!(),
                };
            },
            MicronFormStage::Confirm => {
                if !self.omnibar.text.trim().eq_ignore_ascii_case("send") {
                    return vec![Effect::Redraw];
                }
                let source_is_current = self
                    .graph_runtimes
                    .graph()
                    .get_node_by_id(prompt.source)
                    .is_some_and(|(_, node)| node.url() == prompt.source_url)
                    && self
                        .content
                        .fetched(prompt.source, &prompt.source_url)
                        .is_some_and(|document| document.body == prompt.source_body);
                if !source_is_current {
                    return self.micron_form_error(
                        prompt,
                        "The page changed; close and reopen its form".into(),
                    );
                }
                let prepared = match prompt.form.prepare(prompt.action, &prompt.source_body) {
                    Ok(prepared) => prepared,
                    Err(error) => return self.micron_form_error(prompt, error),
                };
                let Some(target) = self.resolve_micron_form_target(prompt.source, &prepared.target)
                else {
                    return self.micron_form_error(prompt, "This form needs a full native destination or an authoritative same-node target".into());
                };
                if prompt.resolved_target.as_deref() != Some(target.as_str()) {
                    return self.micron_form_error(
                        prompt,
                        "The destination changed; reopen the form".into(),
                    );
                }
                self.next_smolweb_submission = self.next_smolweb_submission.wrapping_add(1);
                let request = self.next_smolweb_submission;
                if let Some(previous) = self.active_smolweb_submission.take() {
                    self.micron_submission_sources.remove(&previous);
                }
                self.active_smolweb_submission = Some(request);
                self.micron_submission_sources.insert(
                    request,
                    (
                        prompt.source,
                        prompt.source_url.clone(),
                        prompt.source_body.clone(),
                    ),
                );
                let bytes = prepared
                    .values()
                    .iter()
                    .map(|(key, value)| key.len() + value.len())
                    .sum();
                self.events.push(AppEvent::SmolwebSubmissionStarted {
                    target: target.clone(),
                    bytes,
                });
                self.omnibar = crate::ui::OmnibarState {
                    open: true,
                    mode: crate::ui::OmnibarMode::SmolwebSubmissionResult(
                        crate::ui::SmolwebSubmissionResult {
                            target: target.clone(),
                            message: "sending".to_string(),
                        },
                    ),
                    ..Default::default()
                };
                self.recompute_omnibar_suggestions();
                return vec![
                    Effect::SubmitMicron {
                        request,
                        source: Some(prompt.source),
                        submission: crate::action::MicronSubmission {
                            target,
                            values: prepared.into_values(),
                        },
                    },
                    Effect::Redraw,
                ];
            },
        }
        self.omnibar.cursor = self.omnibar.text.len();
        self.omnibar.mode = crate::ui::OmnibarMode::MicronForm(prompt);
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    pub(super) fn micron_form_error(
        &mut self,
        mut prompt: crate::ui::MicronFormPrompt,
        error: String,
    ) -> Vec<Effect> {
        prompt.error = Some(error);
        self.omnibar.mode = crate::ui::OmnibarMode::MicronForm(prompt);
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }

    pub(super) fn resolve_micron_form_target(
        &mut self,
        source: Uuid,
        target: &str,
    ) -> Option<String> {
        if crate::nomadnet::parse_address(target).is_some() {
            return Some(target.to_owned());
        }
        let source_url = self.graph_runtimes.graph().get_node_by_id(source)?.1.url();
        let base = crate::nomadnet::parse_address(source_url)?;
        let path = target.strip_prefix(':')?;
        if !path.starts_with('/') || path.contains('\0') {
            return None;
        }
        Some(format!("{}:{path}", base.destination_hex()))
    }

    pub(in super::super) fn begin_smolweb_submission(
        &mut self,
        source: Option<Uuid>,
        target: String,
    ) -> Vec<Effect> {
        let base = source.and_then(|member| {
            self.graph_runtimes
                .graph()
                .get_node_by_id(member)
                .map(|(_, node)| node.url().to_string())
        });
        let resolved = url::Url::parse(&target).or_else(|_| {
            base.as_deref()
                .ok_or(url::ParseError::RelativeUrlWithoutBase)
                .and_then(|base| url::Url::parse(base)?.join(&target))
        });
        let Ok(resolved) = resolved else {
            self.events.push(AppEvent::SmolwebSubmissionFailed {
                target,
                error: "invalid submission target".to_string(),
            });
            return vec![Effect::Redraw];
        };
        let protocol = match resolved.scheme() {
            "titan" => crate::ui::SmolwebSubmissionProtocol::Titan,
            "spartan" => crate::ui::SmolwebSubmissionProtocol::Spartan,
            _ => {
                self.events.push(AppEvent::SmolwebSubmissionFailed {
                    target: resolved.to_string(),
                    error: "submission target must use titan:// or spartan://".to_string(),
                });
                return vec![Effect::Redraw];
            },
        };
        let target_context = self.fallback_shell_context();
        self.shell.begin_omnibar(target_context);
        self.omnibar = crate::ui::OmnibarState {
            open: true,
            mode: crate::ui::OmnibarMode::SmolwebSubmission(crate::ui::SmolwebSubmissionPrompt {
                source,
                target: resolved.to_string(),
                protocol,
                stage: crate::ui::SmolwebSubmissionStage::Body,
                body: Vec::new(),
                file_name: None,
                mime: "text/gemini".to_string(),
                token: None,
            }),
            ..Default::default()
        };
        self.focus = FocusTarget::Chrome;
        self.recompute_omnibar_suggestions();
        self.events.push(AppEvent::SmolwebSubmissionComposed {
            target: resolved.to_string(),
        });
        vec![Effect::Redraw]
    }

    pub(in super::super) fn set_smolweb_submission_file(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        suggested_mime: String,
    ) -> Vec<Effect> {
        const CAP: usize = 64 * 1024 * 1024;
        let crate::ui::OmnibarMode::SmolwebSubmission(mut submission) = self.omnibar.mode.clone()
        else {
            return vec![Effect::Redraw];
        };
        if submission.stage != crate::ui::SmolwebSubmissionStage::Body || bytes.len() > CAP {
            return vec![Effect::Redraw];
        }
        submission.body = bytes;
        submission.file_name = Some(name);
        submission.mime = suggested_mime;
        self.omnibar.text.clear();
        self.omnibar.cursor = 0;
        self.omnibar.mode = crate::ui::OmnibarMode::SmolwebSubmission(submission);
        self.recompute_omnibar_suggestions();
        vec![Effect::Redraw]
    }
}
