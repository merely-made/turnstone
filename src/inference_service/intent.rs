// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) async fn intent(
    connection: &mut Connection,
    name: &str,
    payload: Vec<u8>,
) -> Result<(), String> {
    if name == ASK_PUBLISHED_INTENT {
        crate::inference_activity::record_generation();
    } else if name == PUBLISH_CORPUS_INTENT {
        crate::inference_activity::record_publication();
    }
    let result = connection
        .client
        .request_body(CarrierRequestBody::Intent(IntentInvocation {
            session: connection.request.session.clone(),
            target: InstanceId(0),
            observed_epoch: SceneEpoch(1),
            observed_revision: Revision(0),
            intent: name.into(),
            payload,
        }))
        .await
        .map_err(|e| e.to_string())?;
    match result {
        CarrierResponseBody::Intent(IntentResult::Accepted) => Ok(()),
        CarrierResponseBody::Intent(IntentResult::Rejected { reason }) => Err(reason),
        _ => Err("The resident returned an unexpected answer.".into()),
    }
}
