// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl IntentSink for TurnstoneEndpoint {
    type Error = String;

    fn invoke(&mut self, intent: IntentInvocation) -> Result<IntentResult, Self::Error> {
        if intent.session != self.session {
            return Err("intent names the wrong session".into());
        }
        let Some((epoch, revision)) = self.active_revision() else {
            return Err("intent arrived before a snapshot".into());
        };
        if intent.observed_epoch != epoch || intent.observed_revision != revision {
            return Ok(IntentResult::Stale {
                current_epoch: epoch,
                current_revision: revision,
            });
        }
        if !self.intent_was_advertised(&intent) {
            return Ok(IntentResult::Rejected {
                reason: "target or intent was not disclosed by this snapshot".into(),
            });
        }

        match intent.intent.as_str() {
            FIT_INTENT => {
                if !intent.payload.is_empty() {
                    return Ok(IntentResult::Rejected {
                        reason: "fit-view accepts an empty payload".into(),
                    });
                }
                let audit = Container::new(format!("{LAYOUT_SCOPE}fit-{}", self.audit.revision()))
                    .with_title("Fit disclosed Turnstone graph");
                match self.petition(&layout_scope(), audit) {
                    Ok(()) => {
                        self.app.update(Action::FitView);
                        Ok(IntentResult::Accepted)
                    },
                    Err(error) => Ok(IntentResult::Rejected {
                        reason: format!("authority gate refused fit-view: {error:?}"),
                    }),
                }
            },
            OPEN_INTENT => {
                let Ok(address) = String::from_utf8(intent.payload) else {
                    return Ok(IntentResult::Rejected {
                        reason: "open-address payload is not UTF-8".into(),
                    });
                };
                if address.trim().is_empty() {
                    return Ok(IntentResult::Rejected {
                        reason: "open-address requires an address".into(),
                    });
                }
                let audit = Container::new(format!("{GRAPH_SCOPE}open-{}", self.audit.revision()))
                    .with_title(format!("Open {address}"));
                match self.petition(&graph_scope(), audit) {
                    Ok(()) => {
                        let _restore_author = EndpointAuthorRestore::enter(
                            self.app.journal.clone(),
                            Author::engine(self.subject.to_hex(), env!("CARGO_PKG_VERSION"))
                                .via("turnstone"),
                        );
                        self.app.update(Action::OpenAddress(address));
                        Ok(IntentResult::Accepted)
                    },
                    Err(error) => Ok(IntentResult::Rejected {
                        reason: format!("authority gate refused graph change: {error:?}"),
                    }),
                }
            },
            _ => Ok(IntentResult::Rejected {
                reason: "unknown endpoint intent".into(),
            }),
        }
    }
}
