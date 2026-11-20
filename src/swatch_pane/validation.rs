// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::saved_sources::KeptSource;
use crate::source_capture::SourceCaptureTarget;
use cambium::{DetailRow, DetailSection, ValidationEvidence, ValidationFinding};
use mere::acquire::page_claims::PageClaims;

const MAX_PREVIEW_CLAIMS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Keep(SourceCaptureTarget),
    Validate(KeptSource),
    Reopen(KeptSource),
    OpenEvidence { source: KeptSource, order: u64 },
}

impl Intent {
    pub(crate) fn into_action(self, pane: crate::panes::PaneId) -> crate::action::Action {
        use crate::action::Action;
        match self {
            Self::Keep(target) => Action::CaptureSourceDocument { target },
            Self::Validate(source) => Action::ValidateKeptSource {
                pane,
                source,
                shapes: Shapes::Choose,
            },
            Self::Reopen(source) => Action::ValidateKeptSource {
                pane,
                source,
                shapes: Shapes::Reopen,
            },
            Self::OpenEvidence { source, order } => Action::RevealValidationEvidence {
                pane,
                source,
                order,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shapes {
    Choose,
    File(std::path::PathBuf),
    Reopen,
}

pub struct Response {
    pub generation: Uuid,
    pub request: Uuid,
    pub pane: crate::panes::PaneId,
    pub source: KeptSource,
    pub result: Result<Report, String>,
}

impl SwatchPane {
    pub(crate) fn begin_validation(&mut self, source: &KeptSource, request: Uuid) -> bool {
        let mut started = false;
        self.runner.update(|state| {
            if let Some(selection) = &mut state.neighborhood
                && selection.validation.kept.as_ref() == Some(source)
                && selection.validation.pending.is_none()
            {
                selection.validation.pending = Some(request);
                started = true;
            }
        });
        started
    }

    pub(crate) fn finish_validation(&mut self, response: Response, admitted: bool) {
        self.runner.update(|state| {
            if let Some(selection) = &mut state.neighborhood
                && selection.validation.pending == Some(response.request)
                && selection.validation.kept.as_ref() == Some(&response.source)
            {
                selection.validation.pending = None;
                if admitted {
                    selection.validation.result = Some(response.result);
                    selection.validation.selected = None;
                    selection.validation.open_error = None;
                }
            }
        });
    }

    pub(crate) fn record_evidence_open(&mut self, order: u64, result: Result<(), String>) {
        self.runner.update(|state| {
            if let Some(selection) = &mut state.neighborhood {
                match result {
                    Ok(()) => {
                        selection.validation.selected = Some(order);
                        selection.validation.open_error = None;
                    },
                    Err(error) => selection.validation.open_error = Some(error),
                }
            }
        });
    }

    pub(crate) fn validation_evidence(&self, source: &KeptSource, order: u64) -> Option<&Report> {
        let state = &self.runner.state().neighborhood.as_ref()?.validation;
        if state.kept.as_ref() != Some(source) {
            return None;
        }
        let report = state.result.as_ref()?.as_ref().ok()?;
        report
            .findings
            .iter()
            .any(|entry| entry.evidence.iter().any(|item| item.id == order))
            .then_some(report)
    }
}

pub struct FindingView {
    pub finding: ValidationFinding,
    pub evidence: Vec<ValidationEvidence<u64>>,
}

pub struct Report {
    pub origin: String,
    pub blocks: Vec<fleece::EmbeddedJsonLdBlock>,
    pub conforms: bool,
    pub cursor: u64,
    pub diagnostics: Vec<String>,
    pub claims: Option<DetailSection>,
    pub findings: Vec<FindingView>,
}

#[derive(Default)]
pub(super) struct State {
    member: Option<Uuid>,
    keep: Option<SourceCaptureTarget>,
    pub(super) kept: Option<KeptSource>,
    diagnostics: Vec<String>,
    claims: Option<DetailSection>,
    has_claims: bool,
    capture_status: String,
    pub(super) pending: Option<Uuid>,
    pub(super) result: Option<Result<Report, String>>,
    selected: Option<u64>,
    open_error: Option<String>,
}

impl State {
    pub(super) fn sync(
        &mut self,
        app: &App,
        member: Option<Uuid>,
        page: Option<&super::neighborhood::Page>,
    ) {
        let kept = member.and_then(|member| app.kept_structured_source(member));
        if self.kept != kept || self.member != member {
            self.pending = None;
            self.result = None;
            self.selected = None;
            self.open_error = None;
        }
        self.member = member;
        self.kept = kept;
        let structured = page.and_then(|page| page.structured.as_ref());
        self.has_claims = structured
            .is_some_and(|input| !input.blocks.is_empty() || !input.diagnostics.is_empty());
        self.claims = structured.and_then(claim_preview);
        self.diagnostics = structured
            .into_iter()
            .flat_map(|input| &input.diagnostics)
            .map(|issue| {
                format!(
                    "JSON-LD block {}: {}",
                    issue.order.saturating_add(1),
                    issue.message
                )
            })
            .collect();
        let custody_ready = app.research.is_available();
        self.capture_status = if custody_ready {
            app.source_capture_status(member)
        } else {
            "Unlock the profile to keep or validate a source.".into()
        };
        self.keep = if custody_ready
            && self.has_claims
            && self.kept.is_none()
            && self.capture_status != "Saving"
        {
            member.and_then(|member| app.source_capture_target(member).ok())
        } else {
            None
        };
    }
}

pub(crate) fn claim_preview(claims: &PageClaims) -> Option<DetailSection> {
    let total: usize = claims.blocks.iter().map(|block| block.quads.len()).sum();
    if total == 0 {
        return None;
    }
    let title = if total > MAX_PREVIEW_CLAIMS {
        format!("Interpreted claims · first {MAX_PREVIEW_CLAIMS} of {total}")
    } else {
        "Interpreted claims".into()
    };
    let rows = claims
        .blocks
        .iter()
        .flat_map(|block| {
            block.quads.iter().map(|quad| {
                DetailRow::new(
                    format!("Block {}", block.order.saturating_add(1)),
                    quad.to_string(),
                )
            })
        })
        .take(MAX_PREVIEW_CLAIMS)
        .collect();
    Some(DetailSection::new(title, rows))
}

fn note(text: impl Into<String>) -> SwatchView {
    Box::new(
        cambium::el::<_, SwatchState, ()>("div", text.into())
            .attr("style", "padding: 3px 12px; font-size: 12px;"),
    )
}

fn button(label: &'static str, intent: Intent) -> SwatchView {
    Box::new(cambium::on_click(
        cambium::el::<_, SwatchState, ()>("button", label.to_owned())
            .attr("type", "button")
            .attr("class", "shell-control")
            .attr("style", "margin: 2px 12px;"),
        move |state: &mut SwatchState, _: cambium::PointerClick| {
            state.pending.push(SwatchIntent::Validation(intent.clone()));
        },
    ))
}

pub(super) fn controls(state: &State) -> Vec<SwatchView> {
    if !state.has_claims && state.kept.is_none() {
        return Vec::new();
    }
    let mut rows = vec![note("Structured data")];
    rows.extend(state.diagnostics.iter().cloned().map(note));
    let Some(source) = &state.kept else {
        rows.push(note("Keep this source to validate its claims."));
        if let Some(target) = &state.keep {
            rows.push(button(
                "Keep current page as a source",
                Intent::Keep(target.clone()),
            ));
        }
        if !matches!(state.capture_status.as_str(), "Ready" | "Saved") {
            rows.push(note(state.capture_status.clone()));
        }
        if let Some(claims) = &state.claims {
            rows.push(Box::new(cambium::detail_panel(std::slice::from_ref(
                claims,
            ))));
        }
        return rows;
    };
    if state.pending.is_some() {
        rows.push(note("Validating kept source…"));
    } else {
        rows.push(button(
            "Validate with shapes…",
            Intent::Validate(source.clone()),
        ));
        rows.push(button(
            "Reopen validation for this capture",
            Intent::Reopen(source.clone()),
        ));
    }
    match &state.result {
        Some(Err(error)) => rows.push(note(error.clone())),
        Some(Ok(report)) => {
            rows.push(note(format!("Source journal cursor {}", report.cursor)));
            if state.diagnostics.is_empty() {
                rows.extend(report.diagnostics.iter().cloned().map(note));
            }
            if report.conforms {
                rows.push(note("The kept source conforms."));
            }
            for entry in &report.findings {
                let source = source.clone();
                rows.push(Box::new(cambium::validation_finding_panel(
                    &entry.finding,
                    &entry.evidence,
                    state.selected.as_ref(),
                    move |state: &mut SwatchState, order| {
                        state
                            .pending
                            .push(SwatchIntent::Validation(Intent::OpenEvidence {
                                source: source.clone(),
                                order,
                            }));
                    },
                )));
            }
        },
        None => {},
    }
    if let Some(error) = &state.open_error {
        rows.push(note(error.clone()));
    }
    let claims = state
        .claims
        .as_ref()
        .or_else(|| state.result.as_ref()?.as_ref().ok()?.claims.as_ref());
    if let Some(claims) = claims {
        rows.push(Box::new(cambium::detail_panel(std::slice::from_ref(
            claims,
        ))));
    }
    rows
}
