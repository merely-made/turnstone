// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resolve_source(
    owner: &dyn RetainedSourceOwner,
    runs: &[RunV1],
    run: &str,
    handle: u32,
) -> Result<OpenSource, String> {
    let evidence = runs
        .iter()
        .find(|r| r.run == run)
        .and_then(|r| r.recall.as_ref())
        .ok_or("The retained answer is unavailable.")?;
    let citation = evidence
        .citations
        .iter()
        .find(|c| c.handle == handle)
        .ok_or("That citation is stale or unavailable.")?;
    let passage = &evidence
        .passages
        .iter()
        .find(|p| p.handle == handle)
        .ok_or("The cited passage is unavailable.")?
        .passage;
    if citation.passage != passage.id() {
        return Err("The citation does not name its supplied passage.".into());
    }
    if passage.source.kind == eidetic::models::recall::RecallSourceKind::Fleece
        && (evidence.owner != passage.source.owner || !evidence.scope.contains(&passage.source))
    {
        return Err(
            "The cited saved source was outside the explicitly chosen answer scope.".into(),
        );
    }
    let text = owner.resolve(&passage.source, &passage.version)?;
    if passage.selector.start >= passage.selector.end || passage.selector.end > text.chars().count()
    {
        return Err("The cited source range is invalid.".into());
    }
    if eidetic::Hash::of(text.as_bytes()) != passage.text_hash {
        return Err("The retained source text no longer matches the cited version.".into());
    }
    let excerpt: String = text
        .chars()
        .skip(passage.selector.start)
        .take(passage.selector.end.saturating_sub(passage.selector.start))
        .collect();
    if excerpt != passage.text {
        return Err("The retained source range no longer matches the citation.".into());
    }
    Ok(OpenSource {
        run: run.into(),
        handle,
        retained: RetainedSourceText {
            source: passage.source.clone(),
            version: passage.version.clone(),
            title: None,
            text,
        },
        selector: passage.selector.clone(),
    })
}
