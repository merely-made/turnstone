// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Mint the confirmed participant into the session: the graph node, the binding +
/// source facets, the nested world with its gate-projected grant, and the
/// runtime entry. Returns the member id. (The caller persists: facets ride
/// the ordinary save; the nested log saves here, once, at its birth.)
pub fn install(app: &mut App, pending: PendingInstall) -> Result<Uuid, String> {
    // Install is a delegation signed by the profile root; with the vault
    // locked there is none to sign with, and no fallback stands in for it.
    let Some(root) = app.identity.clone() else {
        return Err(PENDING_IDENTITY.to_string());
    };
    if let Some(error) = &app.denizens.admission_error {
        return Err(format!(
            "resident admission state needs owner repair before install: {error}"
        ));
    }
    let persisted_admission = crate::resident_admission::path(&app.session_dir());
    match persisted_admission.try_exists() {
        Ok(true) => {
            crate::resident_admission::load(&app.session_dir())?;
        },
        Ok(false) => {},
        Err(err) => {
            return Err(format!(
                "cannot inspect resident admission state before install: {err}"
            ));
        },
    }
    let subject = pending.subject;
    let deadband = pending.deadband;
    let hex = subject.to_hex();

    // The graph node — minted through the ordinary spine (visit selects it).
    let key = app.graph_runtimes.visit(&denizen_url(subject));
    let member = app
        .graph_runtimes
        .graph()
        .get_node(key)
        .map(|n| n.id)
        .expect("the just-visited node exists");
    let _ = app
        .graph_runtimes
        .set_node_title_for(member, pending.label.clone());
    // The borne world is STRUCTURE: it hangs on the node itself
    // (`Node.nested`, journaled through the delta spine), not on the facet.
    let _ = app
        .graph_runtimes
        .set_node_nested_for(member, Some(LogId::new(hex.clone())));

    // The binding + source + label facets: durable agency truth.
    app.graph_runtimes
        .edit_facets(|facets| {
            pandect::write_denizen_binding(
                facets,
                member,
                &pandect::DenizenBinding::new(hex.clone(), pending.body.kind()),
            );
        })
        .map_err(|error| format!("cannot store participant binding: {error}"))?;
    // The runnable: a script's source rides a facet; a component's bytes ride
    // the disk beside the worlds, with the facet as the pointer.
    match &pending.body {
        PackBody::Scenario(source) => {
            app.graph_runtimes
                .edit_facets(|facets| {
                    facets.set(
                        member,
                        chartulary::FacetId::new(SCENARIO_SOURCE_FACET),
                        serde_json::json!(source),
                        &chartulary::AcceptAll,
                    )
                })
                .map_err(|error| format!("cannot store participant source: {error}"))?
                .map_err(|error| format!("participant source facet refused: {error}"))?;
        },
        PackBody::Component(bytes) => {
            let file = format!("{hex}.wasm");
            let target = component_path(&app.session_dir(), &file);
            let written = (|| -> std::io::Result<()> {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, bytes)
            })();
            match written {
                Ok(()) => {
                    app.graph_runtimes
                        .edit_facets(|facets| {
                            facets.set(
                                member,
                                chartulary::FacetId::new(COMPONENT_FACET),
                                serde_json::json!(file),
                                &chartulary::AcceptAll,
                            )
                        })
                        .map_err(|error| format!("cannot store component reference: {error}"))?
                        .map_err(|error| format!("component reference facet refused: {error}"))?;
                },
                Err(err) => {
                    tracing::warn!(%err, path = ?target, "failed to store the component");
                },
            }
        },
    }
    app.graph_runtimes
        .edit_facets(|facets| {
            facets.set(
                member,
                chartulary::FacetId::new("scenario.label"),
                serde_json::json!(pending.label),
                &chartulary::AcceptAll,
            )
        })
        .map_err(|error| format!("cannot store participant label: {error}"))?
        .map_err(|error| format!("participant label facet refused: {error}"))?;

    // The nested world: fresh log, every granted path projected by the gate
    // (read-only, gate-authored — the browsable record authority derives
    // from). What is granted is exactly what the review named: the participant's
    // own world, the read face, and ONE PATH PER PRESELECTED RING. No blanket
    // `app/` grant — an unnamed ring is an ungranted ring, and the session
    // ring only appears here if the review asked for it.
    // Resolve the declared watch to a concrete region. `visit` mints the
    // target if it is not there yet, so watching a folder before it exists is
    // ordinary rather than a failure: the folder appears, and the watch is
    // real from the first frame.
    // An `app/...` declaration is an event scope, not an address: nothing to
    // resolve and nothing to mint. The prefix is the whole distinction, which
    // is why graph scopes are UUID paths and this one is a reserved word.
    // `every/<period>` is a schedule: no scope, no address, nothing to
    // resolve. It needs no capability either, and that asymmetry is
    // deliberate: being woken by a region reveals that the region changed,
    // while being woken by the clock reveals nothing. What a schedule costs is
    // resource, and the gate for that is the review naming the period.
    let schedule: Option<servitor::Period> = pending
        .watch_url
        .as_ref()
        .and_then(|value| value.strip_prefix("every/"))
        .and_then(servitor::Period::parse);

    let app_watch: Option<servitor::ScopePath> = pending
        .watch_url
        .as_ref()
        .filter(|_| schedule.is_none())
        .filter(|value| {
            *value == crate::behaviors::APP_SCOPE_ROOT
                || value.starts_with(&format!("{}/", crate::behaviors::APP_SCOPE_ROOT))
        })
        .and_then(|value| servitor::ScopePath::parse(value).ok());

    let watched: Option<(Cap, servitor::ScopePath)> = pending
        .watch_url
        .as_ref()
        .filter(|_| app_watch.is_none() && schedule.is_none())
        .map(|url| {
            let key = app.graph_runtimes.visit(url);
            let id = app
                .graph_runtimes
                .graph()
                .get_node(key)
                .map(|node| node.id.to_string())
                .unwrap_or_else(|| url.clone());
            let scope =
                servitor::ScopePath::parse(&id).unwrap_or_else(|_| servitor::ScopePath::root());
            (Cap::Scope(scope.clone()), scope)
        });

    let mut nested = GraphLog::with_id(LogId::new(hex.clone()));
    let app_cap = app_watch.clone().map(Cap::Scope);
    let caps = install_caps(
        &pending.rings,
        watched.as_ref().map(|(cap, _)| cap).or(app_cap.as_ref()),
    );
    for (cap, mode) in &caps {
        let grant = Grant::new(subject, cap.clone(), *mode);
        if let Err(err) = app.denizens.gate.project_grant(&mut nested, &grant) {
            tracing::warn!(?err, "failed to project an install grant");
        }
    }
    save_nested(&app.session_dir(), &hex, &nested);

    // The AUTHORITY: root delegation certificates signed by the profile
    // identity (capability-model C4). Install is an attenuating delegation
    // from the user, so uninstall is revoking it and nothing else.
    let previous = app.denizens.admissions.get(member).cloned();
    let generation = if let Some(previous) = &previous {
        let previous_generation = previous.binding.generation;
        previous_generation
            .checked_add(1)
            .ok_or_else(|| "participant lifecycle generation exhausted".to_string())?
    } else {
        0
    };
    let revision = match &pending.body {
        PackBody::Scenario(source) => crate::resident_admission::body_revision(source.as_bytes()),
        PackBody::Component(bytes) => crate::resident_admission::body_revision(bytes),
    };
    let issued_at = now_ms();
    let certs = issue_install_certificates_for_generation(
        root.as_ref(),
        subject,
        &caps,
        issued_at,
        generation,
    );
    let mut probe = DelegationTable::new(root.master_public_key().to_bytes());
    probe.set_now(issued_at);
    for cert in certs.iter().cloned() {
        probe.adopt(cert);
    }
    if caps
        .iter()
        .any(|(cap, mode)| !servitor::AuthorityProvider::covers(&probe, subject, cap, *mode))
    {
        return Err("new participant certificates did not verify under the current profile".into());
    }
    let mut admissions = app.denizens.admissions.clone();
    let mut record =
        crate::resident_admission::ResidentAdmissions::active(member, subject, revision);
    record.binding.generation = generation;
    // A rebind first records denial under the new generation. This closes the
    // old body/grant before certificate replacement; any later failure leaves
    // a durable refusal rather than a partly enabled reinstall.
    let mut denied = record.clone();
    denied.binding.lifecycle = Lifecycle::Revoked;
    admissions.insert(member, denied.clone());
    crate::resident_admission::save(&app.session_dir(), &admissions)?;
    app.denizens.admissions = admissions.clone();
    if let Some(resident) = app.denizens.residents.get_mut(&member) {
        resident.binding = denied.binding;
    }
    admissions.insert(member, record.clone());
    // Write a reviewed fresh certificate while the prior lifecycle remains
    // revoked/paused. Only after that succeeds may active lifecycle become
    // durable; a failure leaves the previous denial in force.
    save_certs_strict(&app.session_dir(), &hex, &certs)?;
    crate::resident_admission::save(&app.session_dir(), &admissions)?;
    app.denizens.admissions = admissions;
    app.denizens.authority.set_now(issued_at);
    for cert in certs {
        app.denizens.authority.adopt(cert);
    }
    app.denizens.residents.insert(
        member,
        Resident {
            subject,
            label: pending.label,
            nested,
            binding: record.binding,
        },
    );

    // The watch, registered only now: the certificates above are what make the
    // containment check pass, so registering earlier would refuse its own
    // grant. The author label is this journal's convention (the full hex, as
    // `remote_projection` writes it), not chartulary's shorter one.
    if let Some((_, scope)) = watched {
        let authority = &app.denizens.authority;
        match app
            .watches
            .register(authority, subject, scope.clone(), subject.to_hex())
        {
            Ok(watch) => tracing::info!(scope = %watch.scope, "participant watch registered"),
            Err(err) => tracing::warn!(%err, %scope, "participant watch refused"),
        }
    }
    if let Some(period) = schedule {
        // Phase starts now, so a freshly admitted behavior waits out a full
        // period rather than treating its own install as the first tick.
        let started = app.now_ms.unwrap_or_default();
        app.time_watches.register(subject, period, started);
        tracing::info!(%period, "participant schedule registered");
    }
    if let Some(scope) = app_watch {
        let authority = &app.denizens.authority;
        match app
            .app_watches
            .register(authority, subject, scope.clone(), subject.to_hex())
        {
            Ok(watch) => tracing::info!(scope = %watch.scope, "participant app watch registered"),
            Err(err) => tracing::warn!(%err, %scope, "participant app watch refused"),
        }
    }
    if let Some(deadband) = deadband {
        app.deadbands.register(subject, deadband);
        tracing::info!(
            minimum_change = deadband.minimum_change(),
            minimum_interval_ms = deadband.minimum_interval_ms(),
            "participant deadband registered"
        );
    }
    save_watches(
        &app.session_dir(),
        &app.watches,
        &app.app_watches,
        &app.time_watches,
        &app.deadbands,
    );
    Ok(member)
}
