// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) async fn read_runs(connection: &mut Connection) -> Result<InferenceRunsV1, String> {
    let snapshot = connection
        .client
        .snapshot(connection.request.clone())
        .await
        .map_err(|e| e.to_string())?;
    let offer = snapshot
        .presentation
        .offers
        .values()
        .flatten()
        .next()
        .ok_or("The answer card is missing.")?;
    let bytes = connection
        .client
        .resource(snapshot.session.clone(), offer.resource)
        .await
        .map_err(|e| e.to_string())?;
    let card: PortableCardV1 = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let media = *card
        .media
        .first()
        .ok_or("The answer records are missing.")?;
    let bytes = connection
        .client
        .resource(snapshot.session, media)
        .await
        .map_err(|e| e.to_string())?;
    let table: InferenceRunsV1 = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if table.schema != wire::INFERENCE_RUNS_SCHEMA {
        return Err("The resident uses an unsupported answer format.".into());
    }
    Ok(table)
}
