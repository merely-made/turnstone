// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
//! Exact admitted source reads and bounded explicit comparison.
use crate::research::{self, Context, Target};
use eidetic::models::recall::RetainedSourceText;
use eidetic_search::compare::{self, Comparison, ComparisonRelationship, ComparisonSettings};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: uuid::Uuid,
    pub context: Context,
    pub revision: u64,
    pub epoch: u64,
    pub left: Target,
    pub right: Target,
    pub settings: ComparisonSettings,
}
#[derive(Clone, Debug)]
pub struct ResultView {
    pub left: RetainedSourceText,
    pub right: RetainedSourceText,
    pub comparison: Comparison,
}
pub(crate) struct Response {
    pub request: Request,
    pub result: Result<ResultView, crate::research_storage::SourceReadError>,
}
fn read(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<crate::trail_memory::TrailCommand>,
    request: &Request,
    target: &Target,
    budget: &crate::research_storage::SharedReadBudget,
) -> Result<RetainedSourceText, crate::research_storage::SourceReadError> {
    if target.selector().is_some() {
        return Err("A comparison side must name a whole source.".into());
    }
    let (title, text) = match target {
        Target::Capture(target) => {
            let (reply, receive) = mpsc::sync_channel(1);
            if !trail.command(crate::trail_memory::TrailCommand::ResearchSource {
                context: request.context.clone(),
                target: target.clone(),
                budget: Some(budget.clone()),
                reply,
            }) {
                return Err("The saved-source owner is closed.".into());
            }
            let content = receive
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| "The saved-source owner did not answer.")??;
            (content.title, content.text)
        },
        Target::Clip(target) => {
            let view = bridge.clip_view_bounded(&request.context, &target.clip, budget.clone())?;
            let crate::retained_clip::ClipBody::Text(text) = view.body else {
                return Err("This clip has no text".into());
            };
            (view.clip.title, text)
        },
    };
    if eidetic::Hash::of(text.as_bytes()).to_string() != target.text_hash() {
        return Err("The retained text version changed.".into());
    }
    Ok(RetainedSourceText {
        source: research::source_id(target)?,
        version: target.text_hash().into(),
        title: Some(title),
        text,
    })
}
pub(crate) fn run(
    bridge: &research::Bridge,
    trail: &armillary::ActorHandle<crate::trail_memory::TrailCommand>,
    request: &Request,
) -> Result<ResultView, crate::research_storage::SourceReadError> {
    let check = || -> Result<(), String> {
        if bridge.context_revision()? != (request.context.clone(), request.revision) {
            return Err("Comparison admissions changed.".into());
        }
        if (bridge.clip_access()?.epoch)() != Some(request.epoch) {
            return Err("Comparison custody changed.".into());
        }
        Ok(())
    };
    check()?;
    if !research::admitted(&request.context, &request.left) {
        return Err("Left withdrawn from scope.".into());
    }
    if !research::admitted(&request.context, &request.right) {
        return Err("Right withdrawn from scope.".into());
    }
    let budget = bridge.storage_limits()?.budget();
    let left = read(bridge, trail, request, &request.left, &budget)
        .map_err(|error| error.context("Left unavailable"))?;
    check()?;
    let right = read(bridge, trail, request, &request.right, &budget)
        .map_err(|error| error.context("Right unavailable"))?;
    check()?;
    let comparison = compare::compare(
        &left,
        &right,
        ComparisonRelationship::Explicit,
        &request.settings,
    )
    .map_err(|e| format!("Comparison refused: {e:?}"))?;
    check()?;
    Ok(ResultView {
        left,
        right,
        comparison,
    })
}
