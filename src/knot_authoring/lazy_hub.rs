// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
pub(super) enum HubSender {
    Direct(Sender<HubCommand>),
    Deferred(crate::service_init::ServiceInit<Arc<KnotHub>>),
}
impl From<Sender<HubCommand>> for HubSender {
    fn from(s: Sender<HubCommand>) -> Self {
        Self::Direct(s)
    }
}
impl HubSender {
    pub(super) fn send(&self, command: HubCommand) -> Result<(), String> {
        match self {
            Self::Direct(s) => s.send(command).map_err(|_| "Knot endpoint stopped".into()),
            Self::Deferred(s) => std::sync::LazyLock::force(s)
                .as_ref()
                .map_err(Clone::clone)?
                .commands
                .send(command),
        }
    }
}
impl KnotHub {
    fn configured(
        directory: Option<PathBuf>,
        factory: impl FnOnce() -> Result<Arc<KnotHub>, String> + Send + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            commands: HubSender::Deferred(crate::service_init::ServiceInit::new(Box::new(factory))),
            next_registration: AtomicU64::new(1),
            directory,
        })
    }
    pub(super) fn host(
        source: HostedKnot,
        effects: Option<HostedEffects>,
        evidence: Option<HostedEvidence>,
        limit: u64,
        wake: Wake,
    ) -> Result<(Arc<Self>, Option<crate::publish_service::PublishSource>), String> {
        let directory = match &source {
            HostedKnot::Directory { root } => Some(root.clone()),
        };
        Ok((
            Self::configured(directory, move || {
                Self::start_host(source, effects, evidence, limit, wake).map(|(hub, _)| hub)
            }),
            None,
        ))
    }
    pub(super) fn resident(wake: Wake) -> Result<Arc<Self>, String> {
        Ok(Self::configured(None, move || Self::start_resident(wake)))
    }
    pub(super) fn connect(
        path: PathBuf,
        args: Vec<OsString>,
        wake: Wake,
    ) -> Result<Arc<Self>, String> {
        Ok(Self::configured(None, move || {
            Self::start_connect(path, args, wake)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_authoring_endpoint_is_dormant_until_first_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("first.djot");
        std::fs::write(&path, "# First document\n").unwrap();
        let (hub, publication) = KnotHub::host(
            HostedKnot::Directory {
                root: directory.path().to_path_buf(),
            },
            None,
            None,
            65536,
            Arc::new(|| {}),
        )
        .unwrap();
        assert!(publication.is_none());
        let HubSender::Deferred(resource) = &hub.commands else {
            panic!("configuration started an endpoint")
        };
        assert!(std::sync::LazyLock::get(resource).is_none());
        let address = url::Url::from_file_path(path.canonicalize().unwrap())
            .unwrap()
            .to_string();
        let opened = hub.open(&address).unwrap();
        assert!(std::sync::LazyLock::get(resource).is_some());
        assert!(opened.registration > 0);
    }
}
