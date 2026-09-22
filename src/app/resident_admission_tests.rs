// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

use crate::action::Action;
use crate::observe::AppEvent;
use servitor::resident::Lifecycle;

fn installed_app(name: &str, source: &str) -> (App, uuid::Uuid) {
    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!(
        "turnstone-resident-admission-app-{}-{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join(format!("{name}.lua"));
    std::fs::write(&pack, source).unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let member = *app.denizens.residents.keys().next().unwrap();
    (app, member)
}

#[test]
fn pause_skips_manual_runs_and_resume_is_durable() {
    let (mut app, member) = installed_app("pause", "mere.open('mere://pause')");
    app.set_denizen_paused(member, true).unwrap();

    app.update(Action::RunDenizen { member });
    assert!(app.take_events().iter().any(|event| {
        matches!(event, AppEvent::DenizenRefused(reason) if reason.contains("paused; trigger skipped"))
    }));
    let paused = crate::resident_admission::load(&app.session_dir()).unwrap();
    let record = paused.get(member).unwrap();
    assert_eq!(record.binding.lifecycle, Lifecycle::Paused);
    assert_eq!(record.skipped, 1);

    let reopened = crate::denizen::rebuild(
        app.graph_runtimes.facets(),
        app.graph_runtimes.graph(),
        &app.session_dir(),
        app.identity.as_ref(),
    );
    app.denizens = reopened;
    app.update(Action::RunDenizen { member });
    assert!(app.take_events().iter().any(|event| {
        matches!(event, AppEvent::DenizenRefused(reason) if reason.contains("paused; trigger skipped"))
    }));
    let reopened = crate::resident_admission::load(&app.session_dir()).unwrap();
    assert_eq!(
        reopened.get(member).unwrap().binding.lifecycle,
        Lifecycle::Paused
    );
    assert_eq!(reopened.get(member).unwrap().skipped, 2);
    app.set_denizen_paused(member, false).unwrap();
    let resumed = crate::resident_admission::load(&app.session_dir()).unwrap();
    assert_eq!(
        resumed.get(member).unwrap().binding.lifecycle,
        Lifecycle::Active
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[test]
fn uninstall_persists_revocation_and_rebuild_rejects_lagging_binding() {
    let (mut app, member) = installed_app("revoke", "mere.open('mere://revoke')");
    let binding = pandect::read_denizen_binding(app.graph_runtimes.facets(), member).unwrap();
    app.update(Action::UninstallDenizen { member });

    let revoked = crate::resident_admission::load(&app.session_dir()).unwrap();
    assert_eq!(
        revoked.get(member).unwrap().binding.lifecycle,
        Lifecycle::Revoked
    );
    pandect::write_denizen_binding(app.graph_runtimes.facets_mut(), member, &binding);
    let rebuilt = crate::denizen::rebuild(
        app.graph_runtimes.facets(),
        app.graph_runtimes.graph(),
        &app.session_dir(),
        app.identity.as_ref(),
    );
    assert_eq!(
        rebuilt.residents.get(&member).unwrap().binding.lifecycle,
        Lifecycle::Revoked
    );
    assert_eq!(
        rebuilt.admissions.get(member).unwrap().binding.lifecycle,
        Lifecycle::Revoked
    );
    app.denizens = rebuilt;
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://revoke")
            .is_none()
    );
    app.update(Action::RunDenizen { member });
    assert!(app.take_events().iter().any(|event| {
        matches!(event, AppEvent::DenizenRefused(reason) if reason.contains("Revoked"))
    }));
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://revoke")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[test]
fn duplicate_subjects_refuse_manual_routing_without_actions() {
    let (mut app, first) = installed_app("duplicate-first", "mere.open('mere://first')");
    let second_source = app.data_root.join("duplicate-second.lua");
    std::fs::write(&second_source, "mere.open('mere://second')").unwrap();
    app.update(Action::InstallDenizen {
        path: second_source.display().to_string(),
    });
    app.update(Action::ConfirmInstallDenizen);
    let second = *app
        .denizens
        .residents
        .keys()
        .find(|member| **member != first)
        .unwrap();
    let subject = app.denizens.residents[&first].subject;
    app.denizens.residents.get_mut(&second).unwrap().subject = subject;
    app.denizens.residents.get_mut(&second).unwrap().binding.subject = subject;
    app.denizens.admissions.get_mut(second).unwrap().binding.subject = subject;

    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://first")
            .is_none()
    );
    app.update(Action::RunDenizen { member: first });
    assert!(app.take_events().iter().any(|event| {
        matches!(event, AppEvent::DenizenRefused(reason) if reason.contains("subject routing is ambiguous"))
    }));
    assert!(
        app.graph_runtimes
            .graph()
            .get_node_by_url("mere://first")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&app.data_root);
}

#[cfg(feature = "piccolo")]
#[test]
fn revoking_only_the_watch_scope_refuses_the_wake_but_keeps_base_authority() {
    use servitor::AuthorityProvider;

    let mut app = App::test_stub();
    app.data_root = std::env::temp_dir().join(format!(
        "turnstone-resident-admission-scoped-revoke-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&app.data_root);
    std::fs::create_dir_all(app.session_dir()).unwrap();
    let pack = app.data_root.join("scoped-revoke.lua");
    std::fs::write(
        &pack,
        "-- @watch https://example.com/notes/\nmere.write('mere://summary', mere.trigger())",
    )
    .unwrap();
    app.update(Action::InstallDenizen {
        path: pack.display().to_string(),
    });
    if let Some(pending) = app.pending_install.as_mut() {
        pending.rings.push(crate::ring::Ring::Author);
    }
    app.update(Action::ConfirmInstallDenizen);
    let member = *app.denizens.residents.keys().next().unwrap();
    let subject = app.denizens.residents[&member].subject;
    app.update(Action::ReseedLayout);
    let _ = app.take_events();
    let watch_scope = app
        .watches
        .watches()
        .iter()
        .find(|watch| watch.subject == subject)
        .expect("the install registered the declared watch")
        .scope
        .clone();
    let watch_cap = servitor::Cap::Scope(watch_scope);
    let watch_path = servitor::delegation::cap_path(&watch_cap);
    let watch_cert = crate::denizen::load_certs(&app.session_dir(), &subject.to_hex())
        .into_iter()
        .find(|signed| signed.certificate.scope.path_prefix == watch_path)
        .expect("the install persisted the declared watch certificate");
    assert!(app.denizens.authority.covers(
        subject,
        &crate::denizen::read_cap(),
        servitor::Mode::Write
    ));
    assert!(app.denizens.authority.covers(
        subject,
        &crate::denizen::world_cap(),
        servitor::Mode::Write
    ));
    assert!(app
        .denizens
        .authority
        .covers(subject, &watch_cap, servitor::Mode::Read));

    let before_positive = app.journal.lock().unwrap().entries().len();
    app.update(Action::OpenAddress(
        "https://example.com/notes/one".to_string(),
    ));
    let positive_writes = app
        .journal
        .lock()
        .unwrap()
        .entries()
        .iter()
        .skip(before_positive)
        .filter(|entry| entry.author == subject.to_hex())
        .count();
    assert!(
        positive_writes > 0,
        "an authorized watched wake produced participant-attributed work"
    );

    app.denizens.authority.revoke(watch_cert.certificate.id());
    assert!(app.denizens.authority.covers(
        subject,
        &crate::denizen::read_cap(),
        servitor::Mode::Write
    ));
    assert!(app.denizens.authority.covers(
        subject,
        &crate::denizen::world_cap(),
        servitor::Mode::Write
    ));
    assert!(!app
        .denizens
        .authority
        .covers(subject, &watch_cap, servitor::Mode::Read));

    let _ = app.take_events();
    let before_revoked = app.journal.lock().unwrap().entries().len();
    app.update(Action::OpenAddress(
        "https://example.com/notes/two".to_string(),
    ));
    let revoked_writes = app
        .journal
        .lock()
        .unwrap()
        .entries()
        .iter()
        .skip(before_revoked)
        .filter(|entry| entry.author == subject.to_hex())
        .count();
    assert_eq!(revoked_writes, 0, "the revoked wake produced no participant work");
    assert!(app.take_events().iter().any(|event| {
        matches!(event, AppEvent::DenizenRefused(reason)
            if reason.contains("read authority") || reason.contains("admission refused"))
    }));
    let _ = std::fs::remove_dir_all(&app.data_root);
}
