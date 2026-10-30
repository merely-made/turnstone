// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl PendingScenarioWait {
    pub(in super::super) fn file(path: std::path::PathBuf, frames: u32) -> Self {
        Self {
            condition: WaitCondition::File(path),
            remaining: frames.max(1),
        }
    }

    pub(in super::super) fn status(substring: String, frames: u32) -> Self {
        Self {
            condition: WaitCondition::Status(substring),
            remaining: frames.max(1),
        }
    }

    pub(in super::super) fn poll(&mut self, status: &[String]) -> Result<bool, String> {
        let ready = match &self.condition {
            WaitCondition::File(path) => std::fs::metadata(path).is_ok_and(|meta| meta.len() > 0),
            WaitCondition::Status(substring) => status.iter().any(|line| line.contains(substring)),
        };
        if ready {
            return Ok(true);
        }
        if self.remaining == 0 {
            return Err(match &self.condition {
                WaitCondition::File(path) => {
                    format!("wait-file '{}': still missing or empty", path.display())
                },
                WaitCondition::Status(substring) => {
                    format!("wait-status '{substring}': place status is {status:?}")
                },
            });
        }
        self.remaining -= 1;
        Ok(false)
    }
}
