// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
pub struct KnotShareReaderService {
    active: crate::service_init::ServiceInit<ActiveShareReaderService>,
}
impl KnotShareReaderService {
    pub fn start(root: Arc<RootIdentity>) -> Result<Self, String> {
        Ok(Self {
            active: crate::service_init::ServiceInit::new(Box::new(move || {
                ActiveShareReaderService::start(root)
            })),
        })
    }
    pub fn snapshot(&self) -> SharedKnotSnapshot {
        match std::sync::LazyLock::force(&self.active).as_ref() {
            Ok(s) => s.snapshot(),
            Err(e) => SharedKnotSnapshot {
                status: e.clone(),
                ..Default::default()
            },
        }
    }
    pub fn open(&self, ticket: String) {
        if let Ok(s) = std::sync::LazyLock::force(&self.active).as_ref() {
            s.open(ticket)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuring_a_share_reader_does_not_open_a_carrier() {
        let service =
            KnotShareReaderService::start(Arc::new(RootIdentity::from_seed([71; 32]))).unwrap();
        assert!(std::sync::LazyLock::get(&service.active).is_none());
        drop(service);
    }
}
