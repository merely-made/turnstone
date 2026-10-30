// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotHub {
    /// Host the Knot endpoint in this process, over the in-memory carrier.
    ///
    /// The default path. A spawned endpoint cannot exist in a browser tab or
    /// an iOS app, so hosting is what lets the editor reach the targets the
    /// product is aimed at; `connect` remains for the case where an endpoint
    /// really is a separate program.
    ///
    /// Everything past construction is identical, because the carrier seam is
    /// where the two differ and `run_hub` never sees it.
    pub(super) fn start_host(
        source: HostedKnot,
        effects: Option<HostedEffects>,
        evidence: Option<HostedEvidence>,
        max_source_bytes: u64,
        wake: Wake,
    ) -> Result<(Arc<Self>, Option<crate::publish_service::PublishSource>), String> {
        let directory = match &source {
            HostedKnot::Directory { root } => Some(root.clone()),
        };
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) =
            mpsc::sync_channel::<Result<Option<crate::publish_service::PublishSource>, String>>(1);
        std::thread::Builder::new()
            .name("turnstone-knot-authoring".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let grant = knot::KnotWriteGrant::new(max_source_bytes);
                let opened = match source {
                    HostedKnot::Directory { root } => {
                        knot::KnotEndpoint::open_writable(&root, grant)
                            .map_err(|error| format!("could not open the Knot directory: {error}"))
                            .map(|endpoint| {
                                (endpoint, None::<crate::publish_service::PublishSource>)
                            })
                    },
                };
                let retained = opened
                    .and_then(|(mut endpoint, publish_source)| {
                        if let Some(hosted) = effects {
                            endpoint.grant_effects(knot::KnotEffectAuthority::new(hosted.policy));
                        }
                        if let Some(hosted) = evidence {
                            endpoint.grant_clip_evidence(knot::BlobClipEvidenceStore::open(
                                hosted.root,
                                hosted.max_artifact_bytes,
                            )?);
                        }
                        Ok((endpoint, publish_source))
                    })
                    .and_then(|(endpoint, publish_source)| {
                        // Resume is the endpoint's own answer; the carrier has
                        // no business inventing one.
                        let carrier = graphshell_endpoint::local::LocalCarrier::new(
                            endpoint,
                            |endpoint: &mut knot::KnotEndpoint, request| {
                                graphshell_endpoint::ResumableProjectionSource::resume(
                                    endpoint, request,
                                )
                                .map_err(|error| error.to_string())
                            },
                        );
                        RetainedEndpointSession::over(Box::new(carrier), profile)
                            .map(|retained| (retained, publish_source))
                    });
                match retained {
                    Ok((retained, publish_source)) => {
                        let _ = ready_send.send(Ok(publish_source));
                        run_hub(retained, receiver, wake, false);
                    },
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    },
                }
            })
            .map_err(|error| format!("could not start Knot authoring worker: {error}"))?;
        let publish_source = ready_receive
            .recv()
            .map_err(|_| "Knot authoring worker stopped during startup".to_string())??;
        Ok((
            Arc::new(Self {
                commands: commands.into(),
                next_registration: AtomicU64::new(1),
                directory,
            }),
            publish_source,
        ))
    }

    /// Visit a document another mere holds, over an admitted place session.
    ///
    /// The same shape as [`Self::host`], with the carrier at the far end of a
    /// dialed stream instead of an in-process endpoint, and the same
    /// [`run_hub`] behind it: the document surface edits a visited document
    /// with no knowledge that it is visiting.
    ///
    /// The dial runs ON this thread, not before it. The retained session and
    /// its carrier never cross a thread boundary, so the thread that opens
    /// the stream is the thread that reads it until the visit ends.
    #[cfg(feature = "peers")]
    pub(super) fn visit(
        dial: crate::place::lanes::HolderDial,
        wake: Wake,
    ) -> Result<Arc<Self>, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("turnstone-knot-visiting".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let retained = dial.open().and_then(|(carrier, holder)| {
                    tracing::info!(holder = %crate::place::hex32(&holder.to_bytes()), "visiting a place-held document");
                    RetainedEndpointSession::over(Box::new(carrier), profile)
                });
                match retained {
                    Ok(retained) => {
                        let _ = ready_send.send(Ok(()));
                        run_hub(retained, receiver, wake, true);
                    },
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    },
                }
            })
            .map_err(|error| format!("could not start the Knot visiting worker: {error}"))?;
        ready_receive
            .recv()
            .map_err(|_| "the Knot visiting worker stopped while dialing".to_string())??;
        Ok(Arc::new(Self {
            commands: commands.into(),
            next_registration: AtomicU64::new(1),
            directory: None,
        }))
    }

    #[cfg(feature = "endpoint-process")]
    pub(super) fn start_connect(
        program: PathBuf,
        args: Vec<OsString>,
        wake: Wake,
    ) -> Result<Arc<Self>, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("turnstone-knot-authoring".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let retained = graphshell_host::sessions::spawn_endpoint_session(
                    program.as_os_str(),
                    &args,
                    profile,
                );
                match retained {
                    Ok(retained) => {
                        let _ = ready_send.send(Ok(()));
                        run_hub(retained, receiver, wake, false);
                    },
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    },
                }
            })
            .map_err(|error| format!("could not start Knot authoring worker: {error}"))?;
        ready_receive
            .recv()
            .map_err(|_| "Knot authoring worker stopped during startup".to_string())??;
        Ok(Arc::new(Self {
            commands: commands.into(),
            next_registration: AtomicU64::new(1),
            directory: None,
        }))
    }

    /// Connect Turnstone to the resident Knot route. The resident process is
    /// the only owner of persona files, evidence custody, and p2p sync.
    pub(super) fn start_resident(wake: Wake) -> Result<Arc<Self>, String> {
        let (commands, receiver) = mpsc::channel();
        let (ready_send, ready_receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("turnstone-knot-authoring".into())
            .spawn(move || {
                let profile = CapabilityProfile::new([
                    PresentationCapability::EditableText,
                    PresentationCapability::PortableCard,
                ]);
                let retained = (|| {
                    let route = djinn::door::app_admission::AppRouteId::new("knot")
                        .map_err(|error| format!("invalid resident Knot route: {error}"))?;
                    let carrier = graphshell_endpoint::door::projection::AppRouteCarrier::open(
                        djinn::door::app_admission::AppId::new("turnstone"),
                        route,
                    )
                    .map_err(|error| format!("could not open resident Knot route: {error}"))?;
                    RetainedEndpointSession::over(Box::new(carrier), profile)
                        .map_err(|error| error.to_string())
                })();
                match retained {
                    Ok(retained) => {
                        let _ = ready_send.send(Ok(()));
                        run_hub(retained, receiver, wake, false);
                    },
                    Err(error) => {
                        let _ = ready_send.send(Err(error));
                    },
                }
            })
            .map_err(|error| format!("could not start Knot authoring worker: {error}"))?;
        ready_receive
            .recv()
            .map_err(|_| "Knot authoring worker stopped during startup".to_string())??;
        Ok(Arc::new(Self {
            commands: commands.into(),
            next_registration: AtomicU64::new(1),
            directory: None,
        }))
    }

    pub(super) fn open(&self, address: &str) -> Result<OpenedDocument, String> {
        let registration = self.next_registration.fetch_add(1, Ordering::Relaxed);
        let (events_send, events) = mpsc::channel();
        let (reply_send, reply) = mpsc::sync_channel(1);
        self.commands
            .send(HubCommand::Open {
                registration,
                address: address.to_string(),
                events: events_send,
                reply: reply_send,
            })
            .map_err(|_| "Knot authoring worker is unavailable".to_string())?;
        let (confirmed_registration, binding) = reply
            .recv()
            .map_err(|_| "Knot authoring worker stopped while opening".to_string())??;
        debug_assert_eq!(registration, confirmed_registration);
        Ok(OpenedDocument {
            registration,
            binding,
            events,
        })
    }
}

#[cfg(not(feature = "endpoint-process"))]
impl KnotHub {
    pub(super) fn start_connect(
        _: PathBuf,
        _: Vec<OsString>,
        _: Wake,
    ) -> Result<Arc<Self>, String> {
        Err("Process endpoints require the endpoint-process feature".into())
    }
}
