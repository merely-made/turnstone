// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl crate::inference_service::RetainedSourceOwner for TrailInferenceOwner {
    fn snapshot(
        &self,
        owner: &str,
    ) -> Result<Vec<eidetic::models::recall::RetainedSourceText>, String> {
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.0.command(TrailCommand::InferenceSnapshot {
            owner: owner.into(),
            reply,
        }) {
            return Err("The retained-text owner is closed.".into());
        }
        receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The retained-text owner did not answer.".to_string())?
    }
    fn resolve(
        &self,
        source: &eidetic::models::recall::RecallSourceId,
        version: &str,
    ) -> Result<String, String> {
        let (reply, receive) = std::sync::mpsc::sync_channel(1);
        if !self.0.command(TrailCommand::InferenceSource {
            source: source.clone(),
            version: version.into(),
            reply,
        }) {
            return Err("The retained-text owner is closed.".into());
        }
        receive
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "The retained-text owner did not answer.".to_string())?
    }
}
