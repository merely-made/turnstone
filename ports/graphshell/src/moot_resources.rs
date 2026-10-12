// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0
//! Retained owner bindings for independently mounted resource/coop readings.
use moot::resource_coop::control::{Controller, Intent};
use moot::resource_coop::{Outcome, Request, Snapshot, Target, validate_request};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct Command {
    pub reading: u32,
    pub token: u32,
    pub reader: [u8; 32],
    pub target: serde_json::Value,
}
struct Pending {
    reading: u32,
    request: Request,
    dispatched: bool,
}
#[derive(Default)]
pub struct Host {
    pub readings: BTreeMap<u32, Controller>,
    pub opened: BTreeMap<u32, (moot::resource_coop::ResourceRef, String)>,
    pending: BTreeMap<u32, Pending>,
    queued: Vec<Command>,
    next_token: u32,
    pub version: u64,
}
impl Host {
    /// Awaited local owner requests count as work even while their commands are
    /// outside the scene. Clearing a reading still makes no cancellation claim.
    pub fn in_flight(&self) -> bool {
        self.readings
            .values()
            .any(|reading| reading.pane.pending().is_some())
    }
    pub fn install(&mut self, id: u32, snapshot: Snapshot) -> Result<(), String> {
        if let Some(reading) = self.readings.get_mut(&id) {
            reading.refresh(snapshot).map_err(|e| e.to_string())?;
        } else {
            self.readings
                .insert(id, Controller::new(snapshot).map_err(|e| e.to_string())?);
        }
        self.opened.retain(|reading, (reference, _)| {
            *reading != id
                || self.readings[&id]
                    .pane
                    .snapshot()
                    .resources
                    .iter()
                    .any(|resource| {
                        resource.reference == *reference
                            && matches!(
                                resource.content,
                                moot::resource_coop::ContentRead::Resolved
                            )
                    })
        });
        self.invalidate(id);
        self.version += 1;
        Ok(())
    }
    pub fn close(&mut self, id: u32) {
        self.readings.remove(&id);
        self.opened.remove(&id);
        self.invalidate(id);
        self.version += 1;
    }
    pub fn begin(&mut self, id: u32, intent: Intent) {
        let Some(reading) = self.readings.get_mut(&id) else {
            return;
        };
        let request = reading.begin(intent);
        if self
            .opened
            .get(&id)
            .is_some_and(|(reference, _)| reading.pane.selected_resource() != Some(*reference))
        {
            self.opened.remove(&id);
        }
        if let Some(request) = request {
            let Some(token) = self.next_token.checked_add(1) else {
                reading.pane.complete(
                    &request,
                    Outcome::Refused {
                        reason: "Owner request identifiers exhausted; open a new host".into(),
                    },
                );
                self.version += 1;
                return;
            };
            self.next_token = token;
            let target = match &request.target {
                Target::Refresh { .. } => serde_json::json!({"refresh":true}),
                Target::Resource {
                    reference,
                    action,
                    observed_terms,
                } => {
                    serde_json::json!({"resource":reference,"action":action,"observed_terms":observed_terms})
                },
                Target::Activity { reference, action } => {
                    serde_json::json!({"activity":reference,"action":action})
                },
                Target::CollectionFork { moot, observed } => {
                    serde_json::json!({"moot":moot,"fork_collection":observed})
                },
            };
            self.queued.push(Command {
                reading: id,
                token,
                reader: request.reader,
                target,
            });
            self.pending.insert(
                token,
                Pending {
                    reading: id,
                    request,
                    dispatched: false,
                },
            );
        }
        self.invalidate(id);
        self.version += 1;
    }
    fn invalidate(&mut self, id: u32) {
        self.pending.retain(|_, pending| {
            pending.reading != id
                || self
                    .readings
                    .get(&id)
                    .is_some_and(|reading| reading.pane.is_pending(&pending.request))
        });
        self.queued
            .retain(|command| self.pending.contains_key(&command.token));
    }
    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.queued)
    }
    /// The supplying host obtains a fresh owner snapshot before executing the
    /// intention. Domain dispatch still rechecks authority at the actual operation.
    pub fn prepare_dispatch(
        &mut self,
        id: u32,
        token: u32,
        current: &Snapshot,
    ) -> Result<(), String> {
        let pending = self.pending.get_mut(&token).ok_or("Stale owner request")?;
        if pending.reading != id || pending.dispatched {
            return Err("Wrong reading or already dispatched request".into());
        }
        if !self
            .readings
            .get(&id)
            .is_some_and(|reading| reading.pane.is_pending(&pending.request))
        {
            return Err("Stale reading attention or request".into());
        }
        if let Err(error) = validate_request(&pending.request, current) {
            let reason = error.to_string();
            if let Some(reading) = self.readings.get_mut(&id) {
                reading.pane.complete(
                    &pending.request,
                    Outcome::Refused {
                        reason: reason.clone(),
                    },
                );
            }
            self.pending.remove(&token);
            self.version += 1;
            return Err(reason);
        }
        pending.dispatched = true;
        Ok(())
    }
    pub fn complete(&mut self, id: u32, token: u32, outcome: Outcome) -> bool {
        let Some(pending) = self.pending.get(&token) else {
            return false;
        };
        if pending.reading != id || !pending.dispatched {
            return false;
        }
        let pending = self.pending.remove(&token).unwrap();
        let accepted = self
            .readings
            .get_mut(&id)
            .is_some_and(|reading| reading.pane.complete(&pending.request, outcome));
        self.version += 1;
        accepted
    }
    /// A failed owner read/transport ends this local request without granting or
    /// dispatching anything. A stale or foreign failure cannot affect a reading.
    pub fn fail(&mut self, id: u32, token: u32, reason: String) -> bool {
        let Some(pending) = self.pending.get(&token) else {
            return false;
        };
        if pending.reading != id {
            return false;
        }
        let pending = self.pending.remove(&token).unwrap();
        let accepted = self.readings.get_mut(&id).is_some_and(|reading| {
            reading
                .pane
                .complete(&pending.request, Outcome::Refused { reason })
        });
        self.queued.retain(|command| command.token != token);
        self.version += 1;
        accepted
    }
    pub fn complete_refresh(
        &mut self,
        id: u32,
        token: u32,
        current: Snapshot,
    ) -> Result<bool, String> {
        let Some(pending) = self.pending.get(&token) else {
            return Ok(false);
        };
        if pending.reading != id || !pending.dispatched {
            return Ok(false);
        }
        let reading = self.readings.get_mut(&id).ok_or("Reading closed")?;
        let accepted = reading
            .pane
            .complete_refresh(&pending.request, current)
            .map_err(|e| e.to_string())?;
        if accepted {
            self.pending.remove(&token);
            self.opened.retain(|key, (reference, _)| {
                *key != id
                    || reading.pane.snapshot().resources.iter().any(|r| {
                        r.reference == *reference
                            && matches!(r.content, moot::resource_coop::ContentRead::Resolved)
                    })
            });
            self.version += 1;
        }
        Ok(accepted)
    }
    /// Text is supplied by the content owner after its successful verification,
    /// and belongs only to the exact still-live Open request. It is transient.
    pub fn complete_open(&mut self, id: u32, token: u32, outcome: Outcome, text: String) -> bool {
        let Some(pending) = self.pending.get(&token) else {
            return false;
        };
        let Target::Resource {
            reference,
            action: moot::resource_coop::ResourceAction::Open,
            ..
        } = pending.request.target
        else {
            return false;
        };
        if !matches!(outcome, Outcome::Accepted { .. }) {
            return false;
        }
        if self.complete(id, token, outcome) {
            self.opened.insert(id, (reference, text));
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moot::resource_coop::*;
    fn read() -> Snapshot {
        Snapshot {
            moot: [1; 32],
            reader: [2; 32],
            collection: None,
            activities: vec![],
            resources: vec![ResourceRead {
                reference: ResourceRef {
                    moot: [1; 32],
                    contribution: [3; 32],
                    manifest: [4; 32],
                },
                title: "Notebook".into(),
                schema: "text/plain".into(),
                content: ContentRead::AddressOnly,
                retention: RetentionRead::Unknown,
                terms: BTreeMap::new(),
                hosting: HostingRead::Unknown,
                offers: [(ResourceAction::Open, Offer::Offered)].into(),
            }],
        }
    }
    #[test]
    fn owner_read_failure_ends_only_the_exact_pending_request() {
        let current = read();
        let mut host = Host::default();
        host.install(1, current.clone()).unwrap();
        host.install(2, current.clone()).unwrap();
        assert!(!host.in_flight());
        host.begin(1, Intent::Refresh);
        assert!(host.in_flight());
        let command = host.take_commands().pop().unwrap();
        assert!(!host.fail(2, command.token, "foreign".into()));
        assert!(host.fail(1, command.token, "Owner read unavailable".into()));
        assert!(!host.in_flight());
        assert!(host.readings[&1].pane.pending().is_none());
        assert!(matches!(
            host.readings[&1].pane.outcome(),
            Some(Outcome::Refused { .. })
        ));
        assert!(host.readings[&2].pane.outcome().is_none());
        assert_eq!(host.readings[&1].pane.snapshot(), &current);
        assert!(!host.fail(1, command.token, "late".into()));
    }
    #[test]
    fn two_readings_require_fresh_owner_reads_and_exact_local_completions() {
        let current = read();
        let mut host = Host::default();
        host.install(1, current.clone()).unwrap();
        host.install(2, current.clone()).unwrap();
        for id in [1, 2] {
            host.begin(id, Intent::InspectResource(current.resources[0].reference));
            host.begin(id, Intent::Resource(ResourceAction::Open));
        }
        let commands = host.take_commands();
        assert_eq!(commands.len(), 2);
        assert!(!host.complete(
            1,
            commands[0].token,
            Outcome::Accepted {
                owner_receipt: "unvalidated".into()
            }
        ));
        host.prepare_dispatch(1, commands[0].token, &current)
            .unwrap();
        assert!(!host.complete(
            2,
            commands[0].token,
            Outcome::Accepted {
                owner_receipt: "foreign".into()
            }
        ));
        assert!(host.complete(
            1,
            commands[0].token,
            Outcome::Refused {
                reason: "quota".into()
            }
        ));
        host.prepare_dispatch(2, commands[1].token, &current)
            .unwrap();
        host.install(2, current.clone()).unwrap();
        assert!(!host.complete(
            2,
            commands[1].token,
            Outcome::Accepted {
                owner_receipt: "late".into()
            }
        ));
        assert_eq!(host.readings[&1].pane.snapshot(), &current);
        assert!(host.readings[&2].pane.outcome().is_none());
    }
    #[test]
    fn withdrawing_an_offer_refuses_before_owner_dispatch() {
        let mut current = read();
        let mut host = Host::default();
        host.install(1, current.clone()).unwrap();
        host.begin(1, Intent::InspectResource(current.resources[0].reference));
        host.begin(1, Intent::Resource(ResourceAction::Open));
        let command = host.take_commands().pop().unwrap();
        current.resources[0].offers.clear();
        assert!(host.prepare_dispatch(1, command.token, &current).is_err());
        assert!(!host.complete(
            1,
            command.token,
            Outcome::Accepted {
                owner_receipt: "ignored".into()
            }
        ));
        assert!(matches!(
            host.readings[&1].pane.outcome(),
            Some(Outcome::Refused { .. })
        ));
    }

    #[test]
    fn opened_text_and_refresh_completions_stay_with_the_exact_reading() {
        let mut current = read();
        current.resources[0].content = ContentRead::Resolved;
        let mut host = Host::default();
        host.install(1, current.clone()).unwrap();
        host.install(2, current.clone()).unwrap();
        for id in [1, 2] {
            host.begin(id, Intent::InspectResource(current.resources[0].reference));
            host.begin(id, Intent::Resource(ResourceAction::Open));
            let command = host.take_commands().pop().unwrap();
            host.prepare_dispatch(id, command.token, &current).unwrap();
            assert!(host.complete_open(
                id,
                command.token,
                Outcome::Accepted {
                    owner_receipt: "local content owner".into()
                },
                format!("body {id}")
            ));
        }
        host.begin(1, Intent::Refresh);
        let refresh = host.take_commands().pop().unwrap();
        assert!(
            !host
                .complete_refresh(2, refresh.token, current.clone())
                .unwrap()
        );
        current.resources[0].content = ContentRead::Unavailable {
            reason: "Owner no longer discloses this reading".into(),
        };
        host.prepare_dispatch(1, refresh.token, &current).unwrap();
        assert!(host.complete_refresh(1, refresh.token, current).unwrap());
        assert!(!host.opened.contains_key(&1));
        assert_eq!(host.opened[&2].1, "body 2");
        assert!(!host.complete_refresh(1, refresh.token, read()).unwrap());
        host.close(2);
        assert!(host.opened.is_empty());
    }

    #[test]
    fn a_direct_attention_change_also_invalidates_dispatch() {
        let current = read();
        let mut host = Host::default();
        host.install(1, current.clone()).unwrap();
        host.begin(1, Intent::InspectResource(current.resources[0].reference));
        host.begin(1, Intent::Resource(ResourceAction::Open));
        let command = host.take_commands().pop().unwrap();
        host.readings
            .get_mut(&1)
            .unwrap()
            .pane
            .refresh(current.clone())
            .unwrap();
        assert!(host.prepare_dispatch(1, command.token, &current).is_err());
    }
}
