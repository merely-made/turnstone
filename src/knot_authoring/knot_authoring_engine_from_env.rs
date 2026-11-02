// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl KnotAuthoringEngine {
    pub fn from_env(wake: Wake, visits: KnotVisits) -> Result<Option<Self>, String> {
        Self::from_env_in(wake, visits, None)
    }

    pub(crate) fn for_profile(
        wake: Wake,
        visits: KnotVisits,
        profile: &std::path::Path,
    ) -> Result<Option<Self>, String> {
        Self::from_env_in(wake, visits, Some(profile))
    }

    pub(crate) fn notes_root(&self) -> Option<PathBuf> {
        self.hub.as_ref().and_then(|hub| hub.directory.clone())
    }

    pub(super) fn from_env_in(
        wake: Wake,
        visits: KnotVisits,
        profile: Option<&std::path::Path>,
    ) -> Result<Option<Self>, String> {
        let mode =
            std::env::var("TURNSTONE_KNOT_MODE").unwrap_or_else(|_| "directory-write".into());
        if !matches!(mode.as_str(), "directory-write" | "persona-vault") {
            return Err(format!(
                "unsupported TURNSTONE_KNOT_MODE {mode}; expected directory-write or persona-vault"
            ));
        }
        let max_source_bytes = std::env::var("TURNSTONE_KNOT_MAX_BYTES")
            .ok()
            .map(|value| {
                value.parse::<u64>().map_err(|error| {
                    format!("TURNSTONE_KNOT_MAX_BYTES must be an integer: {error}")
                })
            })
            .transpose()?
            .unwrap_or(DEFAULT_MAX_SOURCE_BYTES);
        let default_profile = profile.filter(|_| {
            mode == "directory-write"
                && std::env::var_os("TURNSTONE_KNOT_ROOT").is_none()
                && std::env::var_os("TURNSTONE_KNOT_ENDPOINT").is_none()
        });
        let evidence_root = env_path("TURNSTONE_KNOT_EVIDENCE_ROOT")
            .or_else(|| default_profile.map(|p| p.join("knot-evidence")));
        let max_evidence_bytes = env_integer(
            "TURNSTONE_KNOT_EVIDENCE_MAX_BYTES",
            DEFAULT_MAX_SOURCE_BYTES,
        )?;
        let resolve_mode = effect_mode("TURNSTONE_KNOT_RESOLVE_MODE")?;
        let run_mode = effect_mode("TURNSTONE_KNOT_RUN_MODE")?;
        let schemes = std::env::var("TURNSTONE_KNOT_RESOLVE_SCHEMES").unwrap_or_else(|_| {
            if resolve_mode == "never" {
                String::new()
            } else {
                "file".into()
            }
        });
        let languages = std::env::var("TURNSTONE_KNOT_RUN_LANGUAGES").unwrap_or_else(|_| {
            if run_mode == "never" {
                String::new()
            } else {
                "rhai".into()
            }
        });
        let max_depth = env_integer("TURNSTONE_KNOT_RESOLVE_MAX_DEPTH", DEFAULT_EFFECT_MAX_DEPTH)?;
        let max_ops = env_integer("TURNSTONE_KNOT_RUN_MAX_OPS", DEFAULT_EFFECT_MAX_OPS)?;
        let effects_enabled = resolve_mode != "never" || run_mode != "never";
        let clip_target = std::env::var("TURNSTONE_KNOT_CLIP_TARGET")
            .ok()
            .filter(|target| !target.trim().is_empty());

        if mode == "persona-vault" {
            if env_path("TURNSTONE_KNOT_ENDPOINT").is_some() {
                return Err(
                    "persona-vault mode uses Graphshell's resident Knot route; TURNSTONE_KNOT_ENDPOINT is only valid for isolated directory fixtures"
                        .into(),
                );
            }
            if std::env::var_os("TURNSTONE_KNOT_PERSONA").is_some() {
                return Err(
                    "Graphshell owner settings select the resident Knot persona; remove TURNSTONE_KNOT_PERSONA from Turnstone"
                        .into(),
                );
            }
            if evidence_root.is_some() {
                return Err(
                    "the resident owns Knot evidence custody; TURNSTONE_KNOT_EVIDENCE_ROOT is only valid for directory mode"
                        .into(),
                );
            }
            if effects_enabled {
                return Err(
                    "persona-vault effects require a resident grant; Turnstone cannot grant effects across the app route"
                        .into(),
                );
            }
            return Ok(Some(Self {
                hub: Some(KnotHub::resident(wake)?),
                visits,
                publish_source: None,
                clip_target,
                auto_resolve: false,
                auto_run: false,
            }));
        }

        let Some(root) =
            env_path("TURNSTONE_KNOT_ROOT").or_else(|| default_profile.map(|p| p.join("notes")))
        else {
            // No vault of this profile's own. The engine still registers, so a
            // place-held `knot://<holder>/<path>` address routes to the hub the
            // shell dialed for that holder instead of falling off the end of
            // the content registry into the external-protocol route. Only a
            // LOCAL address has nowhere to go, and that is refused by name.
            return Ok(Some(Self::visit_only(visits, clip_target)));
        };

        if default_profile.is_some() {
            std::fs::create_dir_all(&root)
                .map_err(|e| format!("Could not create the private notes directory: {e}"))?;
        }

        // Host in-process where the endpoint's constructor is a plain
        // directory open. A spawned endpoint cannot exist in a browser tab or
        // an iOS app, so hosting is the path that reaches the targets this
        // product is aimed at, and spawning is the exception rather than the
        // rule.
        //
        // `TURNSTONE_KNOT_ENDPOINT` is an explicit opt-out, for the case where
        // the endpoint genuinely is a separate program. Everything else hosts.
        let explicit_program = env_path("TURNSTONE_KNOT_ENDPOINT").is_some();
        if !explicit_program {
            let effects = effects_enabled.then(|| HostedEffects {
                policy: knot::KnotEffectPolicy {
                    resolve: effect_mode_value(&resolve_mode),
                    run: effect_mode_value(&run_mode),
                    allowed_schemes: split_list(&schemes),
                    allowed_languages: split_list(&languages),
                    max_depth: max_depth as u8,
                    max_ops,
                },
            });
            let evidence = evidence_root.clone().map(|root| HostedEvidence {
                root,
                max_artifact_bytes: max_evidence_bytes,
            });
            reject_nested_evidence_root(&root, evidence_root.as_deref())?;
            let (hub, publish_source) = match KnotHub::host(
                HostedKnot::Directory { root: root.clone() },
                effects,
                evidence,
                max_source_bytes,
                wake,
            ) {
                Ok(hosted) => hosted,
                // A configured vault that will not open is a misconfiguration
                // worth saying out loud, but it is not a reason to strand the
                // place-held addresses this process can still reach. Say it,
                // then carry on as a visitor.
                Err(error) => {
                    tracing::warn!(%error, root = %root.display(), "the configured Knot vault could not be opened; visiting only");
                    return Ok(Some(Self::visit_only(visits, clip_target)));
                },
            };
            return Ok(Some(Self {
                hub: Some(hub),
                visits,
                publish_source,
                clip_target,
                auto_resolve: resolve_mode == "auto",
                auto_run: run_mode == "auto",
            }));
        }

        reject_nested_evidence_root(&root, evidence_root.as_deref())?;
        let endpoint = env_path("TURNSTONE_KNOT_ENDPOINT").ok_or_else(|| {
            "TURNSTONE_KNOT_ENDPOINT disappeared while opening directory mode".to_string()
        })?;
        let endpoint_mode = match (effects_enabled, evidence_root.is_some()) {
            (false, false) => "directory-write",
            (false, true) => "directory-write-evidence",
            (true, false) => "directory-write-effects",
            (true, true) => "directory-write-effects-evidence",
        };
        let mut args = vec![
            endpoint_mode.into(),
            root.into_os_string(),
            max_source_bytes.to_string().into(),
        ];
        if effects_enabled {
            args.extend([
                resolve_mode.clone().into(),
                run_mode.clone().into(),
                schemes.clone().into(),
                languages.clone().into(),
                max_depth.to_string().into(),
                max_ops.to_string().into(),
            ]);
        }
        if let Some(evidence_root) = &evidence_root {
            args.extend([
                evidence_root.clone().into_os_string(),
                max_evidence_bytes.to_string().into(),
            ]);
        }
        let hub = KnotHub::connect(endpoint, args, wake)?;
        Ok(Some(Self {
            hub: Some(hub),
            visits,
            publish_source: None,
            clip_target,
            auto_resolve: resolve_mode == "auto",
            auto_run: run_mode == "auto",
        }))
    }

    /// An engine with no vault of its own: visits only.
    pub(super) fn visit_only(visits: KnotVisits, clip_target: Option<String>) -> Self {
        Self {
            hub: None,
            visits,
            publish_source: None,
            clip_target,
            auto_resolve: false,
            auto_run: false,
        }
    }

    /// The refusal a local Knot address earns on a visit-only engine. One
    /// sentence, naming the missing thing rather than the failed route.
    pub(super) fn no_local_vault(address: &str) -> String {
        format!("no Knot vault is configured, so {address} cannot be opened here")
    }

    pub fn clip_handle(&self) -> Option<KnotClipHandle> {
        // A clip lands in the local vault. Without one there is nothing to
        // clip into, whatever `TURNSTONE_KNOT_CLIP_TARGET` names.
        let hub = self.hub.clone()?;
        self.clip_target.as_ref().map(|target| KnotClipHandle {
            expected_base: None,
            hub,
            target: target.clone(),
            status: Arc::new(Mutex::new(KnotClipStatus::Ready)),
        })
    }

    /// Gives the shell the independent read authority needed to host shares.
    /// Directory and spawned-endpoint modes have none: their file or process
    /// boundary cannot safely be widened by this UI.
    pub fn take_publish_source(&mut self) -> Option<crate::publish_service::PublishSource> {
        self.publish_source.take()
    }

    #[cfg(test)]
    pub(super) fn connect_directory(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "directory-write".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: false,
            auto_run: false,
        })
    }

    #[cfg(test)]
    pub(super) fn connect_directory_evidence(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
        evidence_root: impl Into<PathBuf>,
        max_evidence_bytes: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "directory-write-evidence".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                    evidence_root.into().into_os_string(),
                    max_evidence_bytes.to_string().into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: false,
            auto_run: false,
        })
    }

    #[cfg(test)]
    pub(super) fn connect_directory_effects(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
        resolve: &str,
        run: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "directory-write-effects".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                    resolve.into(),
                    run.into(),
                    "file".into(),
                    "rhai".into(),
                    "1".into(),
                    "10000".into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: resolve == "auto",
            auto_run: run == "auto",
        })
    }

    #[cfg(test)]
    pub(super) fn connect_communal_fixture_effects(
        program: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        max_source_bytes: u64,
        run: &str,
    ) -> Result<Self, String> {
        Ok(Self {
            visits: KnotVisits::default(),
            hub: Some(KnotHub::connect(
                program.into(),
                vec![
                    "communal-fixture-effects".into(),
                    root.into().into_os_string(),
                    max_source_bytes.to_string().into(),
                    "never".into(),
                    run.into(),
                    "".into(),
                    "rhai".into(),
                    "1".into(),
                    "10000".into(),
                ],
                Arc::new(|| {}),
            )?),
            publish_source: None,
            clip_target: None,
            auto_resolve: false,
            auto_run: run == "auto",
        })
    }
}
