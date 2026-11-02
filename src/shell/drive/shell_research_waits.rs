// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
use super::*;

impl Shell {
    pub(super) fn scenario_wait_structured(&mut self, frames: u32, refusal: bool) -> Result<(), String> {
        let reached = self.wait_frames(frames, |shell| {
            let status = shell.app.structured_scenario_status();
            if refusal {
                status["limit_refusal"].is_object()
                    || (status["error"].is_string() && status["busy"] == false)
            } else {
                status["structured_idle_ready"] == true || status["error"].is_string()
            }
        });
        let status = self.app.structured_scenario_status();
        let ready = if refusal { status["limit_refusal"].is_object() } else { status["structured_idle_ready"] == true };
        if !reached || !ready {
            return Err(format!("structured wait after {frames} frames: expected {} completion; error={}",
                if refusal { "bound limit refusal" } else { "idle" }, status["error"]));
        }
        Ok(())
    }

    pub(super) fn scenario_wait_comparison(&mut self, frames: u32) -> Result<(), String> {
        if !self.wait_frames(frames, |shell| shell.app.research_compare_observation().is_ok()) {
            return Err(format!("comparison did not become current and ready after {frames} frames"));
        }
        Ok(())
    }
}
