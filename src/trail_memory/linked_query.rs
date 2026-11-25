// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{SourceCustody, inference_owner, sealed_capture};
use crate::{
    linked_queries::{self, Operation, Outcome, Request, Response, Stamp},
    research::Bridge,
    research_storage::{BudgetStore, StorageLimits},
    saved_sources::{Admission, Key},
};
use muniment::{Backend, DirectoryBackend, FjallStore};
use oxrdf::{NamedOrBlankNode, Term};
use std::{collections::BTreeMap, path::Path};
use turnstone_ontology::{AdmittedSource, RecordIndex, selected_profile};
use uuid::Uuid;

mod resident;
#[cfg(test)]
mod tests;

const CATALOG: &str = "linked-queries.json";
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Catalog {
    bindings: BTreeMap<String, Key>,
    views: BTreeMap<String, turnstone_ontology::SavedView>,
}

pub(super) fn run(
    request: Request,
    bridge: &Bridge,
    directory: &Path,
    generation: Uuid,
    custody: Option<&SourceCustody>,
    store: Option<&FjallStore>,
    limits: &Result<StorageLimits, String>,
) -> Response {
    let result = (|| {
        let custody = custody.ok_or("Unlock the profile to use linked queries.")?;
        let current = || {
            if Stamp::current(bridge).ok().as_ref() != Some(&request.stamp)
                || !custody.remains_current(Some(request.stamp.epoch))
            {
                return Err("The selected source scope changed.".to_string());
            }
            Ok(())
        };
        current()?;
        if generation != request.stamp.context.generation
            || inference_owner(directory)? != request.stamp.scope()
        {
            return Err("This query belongs to another session opening.".into());
        }
        let mut store = BudgetStore {
            store: store.ok_or("The retained source store is unavailable.")?,
            budget: limits.as_ref().map_err(Clone::clone)?.budget(),
        };
        let disk =
            DirectoryBackend::open(directory.join("linked-queries")).map_err(|e| e.to_string())?;
        let vault = sporran::WalletSealedBackend::with_sealer(disk, custody.sealer.clone());
        let result = execute(&request, custody, &mut store, &vault, bridge, current)?;
        current()?;
        Ok(result)
    })();
    Response {
        token: request.token,
        stamp: request.stamp,
        result,
    }
}

fn execute(
    request: &Request,
    custody: &SourceCustody,
    store: &mut dyn eidetic::Store,
    vault: &impl Backend,
    bridge: &Bridge,
    current: impl Fn() -> Result<(), String>,
) -> Result<Outcome, String> {
    let profile = selected_profile().map_err(str::to_owned)?;
    let scope = request.stamp.scope();
    if let Some(v) = &request.view {
        v.check_scope(&scope, profile)?;
    }
    let stored = pollster::block_on(vault.get(CATALOG)).map_err(|e| e.to_string())?;
    let mut catalog: Catalog = match stored {
        Some(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
        None => Catalog::default(),
    };
    if matches!(request.operation, Operation::Catalog) {
        return Ok(Outcome {
            names: catalog.views.keys().cloned().collect(),
            ..Default::default()
        });
    }
    let view = match &request.operation {
        Operation::Named(name) => catalog
            .views
            .get(name)
            .ok_or("The named query is unavailable.")?
            .clone(),
        _ => request.view.clone().ok_or("Choose a saved query first.")?,
    };
    view.check_scope(&scope, profile)?;
    let bindings = &mut catalog.bindings;
    let admissions = &request.stamp.context.admissions;
    if let Operation::Bind(keys) = &request.operation {
        let mut chosen = std::collections::BTreeSet::new();
        for key in keys {
            let admission = admissions
                .iter()
                .find(|a| a.key == *key && a.error.is_none() && a.key.is_sealed())
                .ok_or("A selected capture is unavailable.")?;
            let id = linked_queries::source_id(admission);
            if !view.sources.contains(&id) || !chosen.insert(id.clone()) {
                return Err(
                    "Choose one current revision for each selected source in this query.".into(),
                );
            }
            bindings.insert(id, key.clone());
        }
        if chosen.is_empty() {
            return Err("Choose a retained revision first.".into());
        }
    }
    let mut sources = BTreeMap::new();
    let mut targets = BTreeMap::new();
    let mut issues = Vec::new();
    for id in &view.sources {
        current()?;
        let Some(admission) = bindings.get(id).and_then(|key| {
            admissions.iter().find(|a| {
                a.error.is_none()
                    && a.key == *key
                    && a.key.is_sealed()
                    && linked_queries::source_id(a) == *id
            })
        }) else {
            continue;
        };
        match index(admission, store, custody) {
            Ok((revision, index, target)) => {
                sources.insert(id.clone(), (revision, index));
                targets.insert(id.clone(), target);
            },
            Err(reason) => issues.push(format!("{id} · {reason}")),
        }
    }
    let rows = view.evaluate(&scope, profile, |id| {
        sources
            .get(id)
            .map(|(revision, index)| AdmittedSource { revision, index })
    })?;
    current()?;
    if let Operation::Bind(keys) = &request.operation {
        if keys
            .iter()
            .any(|key| !sources.keys().any(|id| bindings.get(id) == Some(key)))
        {
            return Err(issues
                .pop()
                .unwrap_or_else(|| "Cannot bind an unavailable revision.".into()));
        }
        if let Some(existing) = catalog.views.get(&view.name)
            && existing != &view
        {
            return Err("A different query already uses this name. Choose another name.".into());
        }
        catalog.views.insert(view.name.clone(), view.clone());
        let bytes = serde_json::to_vec(&catalog).map_err(|e| e.to_string())?;
        pollster::block_on(vault.put(CATALOG, &bytes)).map_err(|e| e.to_string())?;
    }
    let mut outcome = Outcome {
        rows,
        issues,
        active: Some(view.clone()),
        ..Default::default()
    };
    if let Operation::Open { source, revision } = &request.operation {
        if !sources.get(source).is_some_and(|(id, _)| id == revision) {
            return Err("The linked source revision changed. Run the query again.".into());
        }
        outcome.open = Some(
            targets
                .get(source)
                .cloned()
                .ok_or("The linked source is unavailable.")?,
        );
    }
    if matches!(
        request.operation,
        Operation::Preview(_) | Operation::Share { .. }
    ) {
        if sources.len() != view.sources.len() {
            return Err("Each source must be available before sharing.".into());
        }
        resident::prepare(&mut outcome, request, &view, &sources, bridge)?;
    }
    Ok(outcome)
}

fn index(
    admission: &Admission,
    store: &mut dyn eidetic::Store,
    custody: &SourceCustody,
) -> Result<(String, RecordIndex, crate::saved_sources::Target), String> {
    let (record, raw, bytes) =
        sealed_capture::verified_response(store, custody.sealer.as_ref(), admission)
            .map_err(|e| e.to_string())?;
    if eidetic::Hash::of(&bytes) != raw.0 {
        return Err("The retained response digest differs from its manifest.".into());
    }
    if bytes.len() > turnstone_ontology::MAX_DOCUMENT_BYTES {
        return Err("The kept response exceeds the linked-document size limit.".into());
    }
    let html =
        std::str::from_utf8(&bytes).map_err(|_| "This captured response is not UTF-8 HTML.")?;
    let evidence = record
        .extraction
        .capture
        .evidence
        .as_ref()
        .ok_or("Missing acquisition context.")?;
    let address = url::Url::parse(&evidence.final_source).map_err(|e| e.to_string())?;
    let dom = genet_static_dom::StaticDocument::parse(html);
    let base = crate::swatch_pane::document_base(&dom, &address);
    let blocks = fleece::extract_json_ld_blocks(&dom);
    if blocks.is_empty() {
        return Err("This kept HTML contains no JSON-LD blocks.".into());
    }
    let claims = mere::acquire::page_claims::interpret(
        &turnstone_ontology::linked_context(),
        base.as_str(),
        blocks
            .iter()
            .map(|b| (b.document_order, b.dom_text.as_str())),
    );
    if !claims.diagnostics.is_empty() {
        return Err("A captured JSON-LD block could not be read.".into());
    }
    let statements = claims.blocks.iter().flat_map(|b| &b.quads).filter_map(|q| {
        if !q.graph_name.is_default_graph() {
            return None;
        }
        let (NamedOrBlankNode::NamedNode(s), Term::NamedNode(o)) = (&q.subject, &q.object) else {
            return None;
        };
        Some((s.as_str(), q.predicate.as_str(), o.as_str()))
    });
    let target = crate::saved_sources::Target {
        key: admission.key.clone(),
        text_hash: record.extraction.canonical_text_hash.to_string(),
        selector: None,
    };
    Ok((
        raw.to_string(),
        RecordIndex::new(selected_profile().map_err(str::to_owned)?, statements),
        target,
    ))
}
