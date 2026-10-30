// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KeyboardDispatchQueue {
    pub(super) fn enqueue(&mut self, commands: Vec<String>) -> Result<(), native::WebSurfaceError> {
        self.check_failure()?;
        self.pending.extend(commands);
        Ok(())
    }

    pub(super) fn check_failure(&self) -> Result<(), native::WebSurfaceError> {
        match &self.failure {
            Some(reason) => Err(native::WebSurfaceError::Platform(reason.clone())),
            None => Ok(()),
        }
    }

    pub(super) fn next(&mut self, now: Instant) -> Result<Option<String>, native::WebSurfaceError> {
        self.check_failure()?;
        if let Some(started) = self.in_flight {
            if now.saturating_duration_since(started) >= KEYBOARD_COMPLETION_TIMEOUT {
                self.complete(Err(
                    "Scry keyboard CDP completion timed out without pumping the UI".into(),
                ));
                self.check_failure()?;
            }
            return Ok(None);
        }
        let command = self.pending.pop_front();
        if command.is_some() {
            self.in_flight = Some(now);
        }
        Ok(command)
    }

    pub(super) fn complete(&mut self, result: Result<(), String>) {
        // A timed-out or retired request cannot revive a failed stream.
        if self.in_flight.take().is_none() || self.failure.is_some() {
            return;
        }
        if let Err(reason) = result {
            self.failure = Some(reason);
            self.pending.clear();
        }
    }
}
