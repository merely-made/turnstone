// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Drive one endpoint session for as long as anyone is reading it.
///
/// `visiting` changes exactly one thing: what a lost endpoint means. A local
/// endpoint that stops is revoked and its document is gone; a holder that
/// stops is unavailable and the last scene stays on screen.
pub(super) fn run_hub(
    mut retained: RetainedEndpointSession,
    commands: Receiver<HubCommand>,
    wake: Wake,
    visiting: bool,
) {
    let mut mounted: Option<ProjectionSession> = None;
    let mut bindings = BTreeMap::<String, DocumentBinding>::new();
    let mut subscribers = BTreeMap::<u64, Subscriber>::new();
    // Only a visited hub has a holder that can vanish. A local endpoint that
    // stops has already answered the last request it will ever answer.
    let liveness = visiting.then(|| LivenessWatch::start(wake.clone()));
    loop {
        if let Some(watch) = &liveness {
            watch.heard();
        }
        match commands.recv_timeout(POLL_INTERVAL) {
            Ok(HubCommand::Open {
                registration,
                address,
                events,
                reply,
            }) => {
                let result = ensure_binding(&mut retained, &mut mounted, &mut bindings, &address)
                    .map(|binding| {
                        if let Some(watch) = &liveness {
                            watch.listen(events.clone());
                        }
                        subscribers.insert(registration, Subscriber { address, events });
                        (registration, binding)
                    });
                let _ = reply.send(result);
            },
            Ok(HubCommand::Save {
                registration,
                base_token,
                source,
            }) => {
                save_from_subscriber(
                    &mut retained,
                    mounted.as_ref(),
                    &mut bindings,
                    &subscribers,
                    registration,
                    base_token,
                    source,
                    &wake,
                    visiting,
                );
            },
            Ok(HubCommand::InsertClip {
                expected_base,
                address,
                clip,
                status,
            }) => {
                insert_clip(
                    &mut retained,
                    &mut mounted,
                    &mut bindings,
                    &subscribers,
                    &address,
                    expected_base,
                    clip,
                    &status,
                    &wake,
                    visiting,
                );
            },
            Ok(HubCommand::Effect {
                registration,
                kind,
                confirmed,
            }) => {
                invoke_effect(
                    &mut retained,
                    mounted.as_ref(),
                    &mut bindings,
                    &subscribers,
                    registration,
                    kind,
                    confirmed,
                    &wake,
                    visiting,
                );
            },
            Ok(HubCommand::Reload { registration }) => {
                reload_subscriber(
                    &mut retained,
                    mounted.as_ref(),
                    &mut bindings,
                    &subscribers,
                    registration,
                    &wake,
                );
            },
            Ok(HubCommand::Unregister { registration }) => {
                subscribers.remove(&registration);
                if subscribers.is_empty() {
                    if let Some(session) = mounted.take() {
                        retained.forget(&session);
                    }
                    bindings.clear();
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                if mounted.is_some() {
                    match retained.poll_for_change() {
                        Ok(true) => refresh_subscribers(
                            &mut retained,
                            mounted.as_ref().expect("checked"),
                            &mut bindings,
                            &subscribers,
                            &wake,
                            visiting,
                        ),
                        Ok(false) => {},
                        Err(error) => {
                            broadcast(&subscribers, lost(visiting, error), &wake);
                            break;
                        },
                    }
                }
            },
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(liveness);
    let _ = retained.close();
}
