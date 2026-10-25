// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The two tools. `mere_actions` reads; `mere_invoke` writes through the
//! route's own intent grammar and carries the revision the caller observed, so
//! the endpoint's stale check stays the caller's to satisfy.

use chirograph::{
    CarrierError, CarrierRequestBody, CarrierResponseBody, IntentInvocation, IntentResult,
};
use sceno::InstanceId;
use scenotime::{Revision, SceneEpoch};
use serde_json::{Value, json};

use crate::rpc::{self, Era, RpcError};
use crate::{Server, view};

const ACTIONS: &str = "mere_actions";
const INVOKE: &str = "mere_invoke";

impl<C: chirograph::Carrier> Server<C> {
    pub(crate) fn tool_list(&self) -> Vec<Value> {
        let mut tools = vec![json!({
            "name": ACTIONS,
            "title": "List the mere's advertised actions",
            "description": "Every action the route advertises in each projection, with its \
                target instance, intent, payload schema and effect, and the session, epoch and \
                revision they were read at. Advertised is not granted: the owner's grant decides \
                each invocation.",
            "inputSchema": {
                "type": "object",
                "properties": {"projection": {
                    "type": "string",
                    "description": "A projection label or resource URI. Omit for all."
                }},
                "additionalProperties": false
            },
            "annotations": {"readOnlyHint": true, "openWorldHint": false}
        })];
        if self.config.writes {
            tools.push(json!({
                "name": INVOKE,
                "title": "Invoke an action on the mere",
                "description": "Invoke one intent on one instance as the app this server was \
                    admitted as. Pass the epoch and revision from mere_actions; a stale pair is \
                    refused with the current one so you can re-read. The payload is the JSON the \
                    action's payload schema names. Accepted edits are attributed to this app.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "projection": {"type": "string", "description": "Projection label or resource URI."},
                        "instance": {"type": "integer", "minimum": 0},
                        "intent": {"type": "string"},
                        "payload": {"description": "The intent's payload: a JSON value, or a string sent verbatim."},
                        "observed_epoch": {"type": "integer", "minimum": 0},
                        "observed_revision": {"type": "integer", "minimum": 0}
                    },
                    "required": ["projection", "instance", "intent", "observed_epoch", "observed_revision"],
                    "additionalProperties": false
                },
                "annotations": {"readOnlyHint": false, "idempotentHint": false, "openWorldHint": false}
            }));
        }
        tools
    }

    pub(crate) fn call_tool(&mut self, era: Era, params: &Value) -> Result<Value, RpcError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let outcome = match name {
            ACTIONS => self.actions(&arguments),
            INVOKE if self.config.writes => self.invoke(&arguments),
            _ => return Err(RpcError::invalid_params(format!("Unknown tool: {name}"))),
        };
        // A protocol failure stays one; everything the model can act on is a
        // tool result with `isError`, so it can correct itself.
        let (value, is_error) = match outcome {
            Ok(done) => done,
            Err(Failure::Protocol(error)) => return Err(error),
            Err(Failure::Tool(message)) => (json!({"error": message}), true),
        };
        Ok(rpc::complete(
            era,
            json!({
                "content": [{"type": "text", "text": value.to_string()}],
                "structuredContent": value,
                "isError": is_error,
            }),
        ))
    }

    fn actions(&mut self, arguments: &Value) -> Result<(Value, bool), Failure> {
        let wanted = arguments.get("projection").and_then(Value::as_str);
        let offers = match wanted {
            Some(name) => vec![self.find_projection(name)?],
            None => self.projections()?,
        };
        let mut projections = Vec::new();
        for offer in offers {
            let snapshot = self.snapshot(&offer)?;
            projections.push(json!({
                "projection": offer.label,
                "uri": self.uri_of(&offer.label),
                "session": snapshot.session.0,
                "epoch": snapshot.scene.epoch.0,
                "revision": snapshot.scene.revision.0,
                "actions": view::actions(&snapshot),
            }));
        }
        Ok((json!({"projections": projections}), false))
    }

    fn invoke(&mut self, arguments: &Value) -> Result<(Value, bool), Failure> {
        let text = |key: &str| {
            arguments
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| Failure::Tool(format!("`{key}` must be a string")))
        };
        let number = |key: &str| {
            arguments
                .get(key)
                .and_then(Value::as_u64)
                .ok_or_else(|| Failure::Tool(format!("`{key}` must be a non-negative integer")))
        };
        let (projection, intent) = (text("projection")?, text("intent")?);
        let instance = u32::try_from(number("instance")?)
            .map_err(|_| Failure::Tool("`instance` is out of range".into()))?;
        let (epoch, revision) = (number("observed_epoch")?, number("observed_revision")?);
        let payload = match arguments.get("payload") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::String(raw)) => raw.clone().into_bytes(),
            Some(value) => value.to_string().into_bytes(),
        };
        let offer = self.find_projection(projection)?;
        let invocation = IntentInvocation {
            session: offer.request.session.clone(),
            target: InstanceId(instance),
            observed_epoch: SceneEpoch(epoch),
            observed_revision: Revision(revision),
            intent: intent.to_string(),
            payload,
        };
        match self.carrier.request(CarrierRequestBody::Intent(invocation)) {
            Ok(CarrierResponseBody::Intent(IntentResult::Accepted)) => {
                Ok((json!({"result": "accepted"}), false))
            },
            Ok(CarrierResponseBody::Intent(IntentResult::Rejected { reason })) => {
                Ok((json!({"result": "rejected", "reason": reason}), true))
            },
            Ok(CarrierResponseBody::Intent(IntentResult::Stale {
                current_epoch,
                current_revision,
            })) => Ok((
                json!({
                    "result": "stale",
                    "current_epoch": current_epoch.0,
                    "current_revision": current_revision.0,
                }),
                true,
            )),
            Ok(other) => Err(Failure::Protocol(RpcError::internal(format!(
                "expected an intent result, got {other:?}"
            )))),
            // The endpoint's refusal (an ungranted route, a bad payload) is
            // what the model needs to read.
            Err(CarrierError::Refused(message)) => Err(Failure::Tool(message)),
            Err(error) => Err(Failure::Protocol(RpcError::internal(self.note(error)))),
        }
    }
}

enum Failure {
    Protocol(RpcError),
    Tool(String),
}

impl From<RpcError> for Failure {
    fn from(error: RpcError) -> Self {
        Self::Protocol(error)
    }
}
