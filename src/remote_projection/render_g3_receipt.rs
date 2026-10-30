// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub fn render_g3_receipt() -> Result<String, String> {
    let run = run_g3_canary()?;
    let result_name = |result: &IntentResult| match result {
        IntentResult::Accepted => "Accepted".to_string(),
        IntentResult::Rejected { .. } => "Rejected".to_string(),
        IntentResult::Stale { .. } => "Stale".to_string(),
    };
    let result_detail = |result: &IntentResult, accepted: &str| match result {
        IntentResult::Accepted => accepted.to_string(),
        IntentResult::Rejected { reason } => reason.clone(),
        IntentResult::Stale { .. } => "The client observation was stale.".into(),
    };
    Ok(graphshell_host::view::render_projection_receipt(
        &graphshell_host::view::ProjectionReceiptView {
            eyebrow: "Graphshell · G3 receipt".into(),
            title: "Turnstone truth, projected.".into(),
            lede: "Mere cartography maps the live browser graph into one Scenograph scene. Graphshell resolves the endpoint-owned cards and returns both intents through the same Servitor authority gate.".into(),
            session: run.session.0,
            status: format!(
                "Live · {} nodes · graph revision {}",
                run.graph_nodes_after, run.graph_revision_after
            ),
            presentations: run.presentations,
            layout: Some(run.layout),
            intents: vec![
                graphshell_host::view::IntentReceiptView {
                    label: format!("{} · curation", fit_label()),
                    result: result_name(&run.fit_result),
                    detail: result_detail(
                        &run.fit_result,
                        "The projected layout grant admitted the harmless view action.",
                    ),
                },
                graphshell_host::view::IntentReceiptView {
                    label: "Open address · graph truth".into(),
                    result: result_name(&run.open_result),
                    detail: result_detail(
                        &run.open_result,
                        "The graph-changing action was admitted and lowered through App::update.",
                    ),
                },
            ],
        },
    ))
}
