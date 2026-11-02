// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn resolve_claim_source(
    owner: &dyn RetainedSourceOwner,
    runs: &[RunV1],
    run: &str,
    handle: u32,
    claim: usize,
    link: usize,
) -> Result<OpenSource, String> {
    let mut opened = resolve_source(owner, runs, run, handle)?;
    let evidence = runs
        .iter()
        .find(|r| r.run == run)
        .and_then(|r| r.recall.as_ref())
        .ok_or("The retained answer is unavailable.")?;
    let support = evidence
        .support
        .as_ref()
        .ok_or("Claim support is unavailable.")?;
    // The resident supplies the projection, but the receiving host checks its
    // immutable raw-output and passage binding before using any narrower range.
    if support.raw_output
        != eidetic::ManifestId::of_blob(runs.iter().find(|r| r.run == run).unwrap().text.as_bytes())
        || support.error.is_some()
    {
        return Err("Claim support does not match the retained answer.".into());
    }
    let link = support
        .claims
        .get(claim)
        .and_then(|c| c.evidence.get(link))
        .ok_or("That claim link is unavailable.")?;
    let passage = evidence
        .passages
        .iter()
        .find(|p| p.handle == handle)
        .ok_or("The supplied passage is unavailable.")?;
    let selector = link
        .selector
        .as_ref()
        .ok_or("The claim has no checked quotation.")?;
    if link.handle != handle
        || link.passage != Some(passage.passage.id())
        || link.error.is_some()
        || selector.start < opened.selector.start
        || selector.end > opened.selector.end
        || selector.start >= selector.end
    {
        return Err("The claim quotation does not name its supplied passage range.".into());
    }
    let quote: String = opened
        .retained
        .text
        .chars()
        .skip(selector.start)
        .take(selector.end - selector.start)
        .collect();
    if quote != link.quote {
        return Err("The retained claim quotation no longer matches its source.".into());
    }
    opened.selector = selector.clone();
    Ok(opened)
}
