// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
pub struct KnotPublishingService {
    active: crate::service_init::ServiceInit<ActivePublishingService>,
}
impl KnotPublishingService {
    pub fn start(source: KnotPublishSource, root: Arc<RootIdentity>) -> Result<Self, String> {
        Ok(Self {
            active: crate::service_init::ServiceInit::new(Box::new(move || {
                ActivePublishingService::start(source, root)
            })),
        })
    }
    pub fn snapshot(&self) -> PublishSnapshot {
        match std::sync::LazyLock::force(&self.active).as_ref() {
            Ok(s) => s.snapshot(),
            Err(e) => PublishSnapshot {
                status: e.clone(),
                ..Default::default()
            },
        }
    }
    pub fn refresh(&self) {
        if let Ok(s) = std::sync::LazyLock::force(&self.active).as_ref() {
            s.refresh()
        }
    }
    pub fn select_source(&self, v: String) {
        if let Ok(s) = std::sync::LazyLock::force(&self.active).as_ref() {
            s.select_source(v)
        }
    }
    pub fn unpublish(&self, v: PublicationId) {
        if let Ok(s) = std::sync::LazyLock::force(&self.active).as_ref() {
            s.unpublish(v)
        }
    }
    pub fn issue_selected(&self, v: [u8; 32], at: Option<u64>) {
        if let Ok(s) = std::sync::LazyLock::force(&self.active).as_ref() {
            s.issue_selected(v, at)
        }
    }
    pub fn revoke(&self, v: u64) {
        if let Ok(s) = std::sync::LazyLock::force(&self.active).as_ref() {
            s.revoke(v)
        }
    }
    pub const fn default_share_hours() -> u64 {
        ActivePublishingService::default_share_hours()
    }
}
