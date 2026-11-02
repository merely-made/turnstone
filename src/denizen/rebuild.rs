// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Rebuild the participant runtime from durable truth on adopt: every
/// `denizen.binding` facet names a resident; the graph node's `nested` field
/// names its borne world (structure), whose log loads from disk (or starts
/// empty), and its authority derives from the **grant projections** in that
/// log — the projection is the readable record, the provider the derived
/// index, so authority is never stored twice.
///
/// A binding written before the containment ruling named the world itself
/// (`legacy_nested_log`); such a resident still rebuilds, and the member goes
/// on [`Denizens::legacy_heals`] so the adopt path can move the pointer onto
/// the node and rewrite the facet without it.
///
/// `root` is the profile root's public key, or `None` while the identity is
/// pending (the vault is locked). A pending rebuild adopts no certificate, so
/// every resident fails closed, and it writes nothing: it never imports
/// admission state, which needs a verified chain.
pub fn rebuild(
    app_facets: &pandect::NodeFacetStore,
    graph: &mere::kernel::graph::Graph,
    session_dir: &Path,
    root: Option<[u8; 32]>,
) -> Denizens {
    let mut denizens = root.map_or_else(Denizens::pending, Denizens::new);
    denizens.authority.set_now(now_ms());
    let admission_path = crate::resident_admission::path(session_dir);
    let missing_admission_state = match admission_path.try_exists() {
        Ok(missing) => !missing,
        Err(err) => {
            denizens.admission_error =
                Some(format!("cannot inspect resident admission state: {err}"));
            false
        },
    };
    let mut imported_admission_state = false;
    if !missing_admission_state {
        match crate::resident_admission::load(session_dir) {
            Ok(state) => denizens.admissions = state,
            Err(err) => {
                denizens.admission_error = Some(err.clone());
                tracing::warn!(%err, "participant admission state is corrupt; all residents fail closed");
            },
        }
    }
    for (member, binding) in pandect::read_denizen_bindings(app_facets) {
        let Ok(raw) = hex_to_bytes(&binding.subject) else {
            tracing::warn!(member = %member, "participant binding with unparseable subject; skipped");
            continue;
        };
        let subject = Subject::new(raw);
        let borne = graph
            .get_node_key_by_id(member)
            .and_then(|key| graph.get_node(key))
            .and_then(|node| node.nested.as_ref())
            .map(|log| log.as_str().to_string());
        let log_id = match borne {
            Some(id) => id,
            None if !binding.legacy_nested_log.is_empty() => {
                denizens
                    .legacy_heals
                    .push((member, binding.legacy_nested_log.clone()));
                binding.legacy_nested_log.clone()
            },
            None => {
                tracing::warn!(member = %member, "participant binding on a node bearing no world; skipped");
                continue;
            },
        };
        let nested = load_nested(session_dir, &log_id)
            .unwrap_or_else(|| GraphLog::with_id(LogId::new(log_id.clone())));
        // The AUTHORITY is the signed certificate chain (C4). Adopt what was
        // persisted; validity is re-verified on every read, never trusted
        // because it was stored.
        //
        // A projection is an audit record, never authority that can mint a
        // replacement certificate. Missing, malformed, or re-rooted chains
        // therefore stay refused until an owner explicitly installs again.
        let stored = load_certs(session_dir, &subject.to_hex());
        let verifies = root.is_some_and(|root| {
            let mut probe = servitor::delegation::DelegationTable::new(root);
            for cert in &stored {
                probe.adopt(cert.clone());
            }
            stored.iter().any(|cert| probe.verify_chain(cert).is_ok())
        });
        for cert in stored.into_iter().filter(|_| verifies) {
            denizens.authority.adopt(cert);
        }
        let label = app_facets
            .get(&member, &chartulary::FacetId::new("scenario.label"))
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| log_id[..8.min(log_id.len())].to_string());
        let body = resident_body(app_facets, member, session_dir);
        let revision = body
            .as_deref()
            .map(crate::resident_admission::body_revision);
        // Content-derived legacy bindings must agree with the body that is
        // actually present. A different body never inherits the old grant.
        let body_matches_subject = revision.is_some_and(|revision| revision.0 == raw);
        let binding = match (
            denizens.admissions.get(member),
            revision,
            verifies && body_matches_subject,
        ) {
            (Some(record), Some(revision), _)
                if record.binding.id == crate::resident_admission::resident_id(member)
                    && record.binding.subject == subject
                    && record.binding.revision == revision =>
            {
                record.binding.clone()
            },
            (None, Some(revision), true) if missing_admission_state => {
                imported_admission_state = true;
                let record = crate::resident_admission::ResidentAdmissions::active(
                    member, subject, revision,
                );
                let binding = record.binding.clone();
                denizens.admissions.insert(member, record);
                binding
            },
            (_, Some(revision), _) => {
                crate::resident_admission::ResidentAdmissions::revoked(member, subject, revision)
                    .binding
            },
            (_, None, _) => {
                crate::resident_admission::ResidentAdmissions::revoked(
                    member,
                    subject,
                    BodyRevision([0; 32]),
                )
                .binding
            },
        };
        if !body_matches_subject {
            tracing::warn!(member = %member, "participant body does not match its content-derived subject; refused");
        }
        denizens.residents.insert(
            member,
            Resident {
                subject,
                label,
                nested,
                binding,
            },
        );
    }
    if imported_admission_state
        && let Err(err) = crate::resident_admission::save(session_dir, &denizens.admissions)
    {
        tracing::warn!(%err, "cannot persist imported participant admission state; refusing imported residents");
        for resident in denizens.residents.values_mut() {
            resident.binding.lifecycle = Lifecycle::Revoked;
        }
    }
    denizens
}
