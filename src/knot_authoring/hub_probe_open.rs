// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[cfg(test)]
impl HubProbe {
    pub(crate) fn open(hub: Arc<KnotHub>, address: &str) -> Result<Self, String> {
        let opened = hub.open(address)?;
        Ok(Self { hub, opened })
    }

    /// The source the endpoint last disclosed for this document.
    pub(crate) fn source(&self) -> &str {
        &self.opened.binding.editable.source
    }

    /// The address the endpoint discloses, which for a visited document is
    /// the holder's own and not the one that was asked for.
    pub(crate) fn disclosed_address(&self) -> &str {
        &self.opened.binding.editable.address
    }

    /// Author one revision through the hub's ordinary save path.
    pub(crate) fn save(&mut self, source: &str) -> Result<String, String> {
        self.hub
            .commands
            .send(HubCommand::Save {
                registration: self.opened.registration,
                base_token: self.opened.binding.editable.base_token.clone(),
                source: source.to_string(),
            })
            .map_err(|_| "the hub is gone".to_string())?;
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match self.opened.events.recv_timeout(left) {
                Ok(HubEvent::Saved { source, binding }) => {
                    self.opened.binding = binding;
                    return Ok(source);
                },
                Ok(HubEvent::Stale) => return Err("stale".into()),
                Ok(HubEvent::Rejected(reason) | HubEvent::Revoked(reason)) => return Err(reason),
                Ok(HubEvent::Unavailable(reason)) => {
                    return Err(format!("unavailable: {reason}"));
                },
                Ok(_) => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    /// Wait for the hub to report that the holder is gone.
    pub(crate) fn wait_for_unavailable(&self, budget: Duration) -> Option<String> {
        let deadline = std::time::Instant::now() + budget;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return None;
            }
            match self.opened.events.recv_timeout(left) {
                Ok(HubEvent::Unavailable(reason)) => return Some(reason),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }
}
