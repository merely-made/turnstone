// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Forms for the focused record's advertised actions.
use std::sync::Arc;

use cambium::{Invocation, button, el};
use chirograph::{AdvertisedAction, IntentInvocation, IntentResult, PortableCardV1};
use graphshell_client::{ActionDraft, ActionDraftTarget, ActionForm};
use graphshell_host::handlers::{
    OPEN_ADDRESS_SCHEMA, OpenAddressV1, SYSTEM_DEFAULT_HANDLER, attached_browser_target,
    handler_from_intent, intent_id,
};

use super::{DETAIL_HEIGHT, GraphView, State, worker};
use mere_overmap::route_session::MereRouteSession;

pub(super) const CSS: &str = "
.resident-actions { box-sizing: border-box; overflow: auto; padding: 8px; }
.resident-actions h2, .resident-actions h3, .resident-actions p { margin: 4px 0; }
.resident-actions button { margin: 0 6px 4px 0; }
.resident-actions fieldset { margin: 4px 0 8px; }
";

pub(super) type OpenAddress = Arc<dyn Fn(String) + Send + Sync>;
pub(super) type Offers = (ActionDraftTarget, Vec<AdvertisedAction>);

const STALE: &str = "The record changed. Reopen the action to try again.";

pub(super) struct Command {
    key: String,
    target: ActionDraftTarget,
    action: AdvertisedAction,
    invocation: Invocation,
}

#[derive(Clone)]
pub(super) struct Reply {
    pub(super) notice: String,
    pub(super) address: Option<String>,
}

impl Reply {
    pub(super) fn new(notice: String) -> Self {
        Self {
            notice,
            address: None,
        }
    }

    pub(super) fn from_intent(result: IntentResult, address: Option<String>) -> Self {
        match result {
            IntentResult::Accepted => Self {
                notice: "Request accepted by the owner.".into(),
                address,
            },
            IntentResult::Stale { .. } => Self::new(STALE.into()),
            IntentResult::Rejected { reason } => Self::new(reason),
        }
    }
}

impl Command {
    /// Correlate the exact outgoing intent with its response before dispatch.
    pub(super) fn queue(
        self,
        panel: &mut MereRouteSession,
    ) -> Result<(IntentInvocation, Option<String>), String> {
        let expected = IntentInvocation::from_command(
            &self.invocation,
            &self.action,
            self.target.session.clone(),
            self.target.target,
            self.target.observed_epoch,
            self.target.observed_revision,
        )
        .map_err(|outcome| format!("{outcome:?}"))?;
        let address = web_address(&expected)?;
        panel.invoke_record_command(self.key, self.target, self.action, self.invocation)?;
        Ok((expected, address))
    }
}

fn web_address(intent: &IntentInvocation) -> Result<Option<String>, String> {
    if intent.intent != intent_id(SYSTEM_DEFAULT_HANDLER) {
        return Ok(None);
    }
    let payload: OpenAddressV1 = serde_json::from_slice(&intent.payload)
        .map_err(|error| format!("Invalid browser request: {error}"))?;
    if payload.handler != SYSTEM_DEFAULT_HANDLER {
        return Err("The owner offered an invalid browser handler.".into());
    }
    attached_browser_target(&payload.address)?;
    Ok(Some(payload.address))
}

fn compose(card: &PortableCardV1, draft: &ActionDraft) -> Result<Invocation, String> {
    let action = draft.action();
    let payload = if action.input_form.is_none() && action.payload_schema == OPEN_ADDRESS_SCHEMA {
        let handler = handler_from_intent(&action.intent.0)
            .ok_or("The owner offered an invalid address handler.")?;
        let address = card
            .values
            .iter()
            .find(|value| value.label == "Address")
            .ok_or("The owner no longer discloses the record's address.")?;
        serde_json::to_vec(&OpenAddressV1 {
            address: address.value.clone(),
            handler: handler.into(),
        })
        .map_err(|error| error.to_string())?
    } else {
        action
            .compose_payload(draft.values())
            .map_err(|error| error.to_string())?
    };
    Ok(action.command_invocation(payload))
}

#[derive(Default)]
pub(super) struct Panel {
    pub(super) open: bool,
    pub(super) offers: Option<Offers>,
    form: ActionForm,
}

impl Panel {
    pub(super) fn has_offers(&self) -> bool {
        self.offers
            .as_ref()
            .is_some_and(|(_, actions)| !actions.is_empty())
    }

    pub(super) fn close(&mut self) {
        self.open = false;
        self.form = ActionForm::default();
    }

    pub(super) fn finish(&mut self, notice: String) {
        self.form.close();
        self.form.status = notice;
    }

    fn still_offered(&self, target: &ActionDraftTarget, action: &AdvertisedAction) -> bool {
        self.offers
            .as_ref()
            .is_some_and(|(current, actions)| current == target && actions.contains(action))
    }

    fn stale(&mut self) {
        self.form.close();
        self.form.status = STALE.into();
    }
}

impl State {
    fn begin_action(&mut self, key: &str, target: &ActionDraftTarget, action: &AdvertisedAction) {
        if self.requested != self.completed {
            return;
        }
        if self.focused.as_deref() != Some(key) || !self.actions.still_offered(target, action) {
            self.actions.stale();
            return;
        }
        self.actions.form.status.clear();
        let bounded = self.actions.form.open_action(
            target.session.clone(),
            target.target,
            action.clone(),
            (target.observed_epoch, target.observed_revision),
        );
        if !bounded {
            self.submit_action();
        }
    }

    fn submit_action(&mut self) {
        if self.requested != self.completed {
            return;
        }
        let (Some(key), Some(card), Some(target), Some(draft)) = (
            &self.focused,
            &self.record,
            &self.actions.form.target,
            &self.actions.form.draft,
        ) else {
            return;
        };
        if !self.actions.still_offered(target, draft.action()) {
            self.actions.stale();
            return;
        }
        match compose(card, draft) {
            Ok(invocation) => {
                let request = worker::Request::Action(Box::new(Command {
                    key: key.clone(),
                    target: target.clone(),
                    action: draft.action().clone(),
                    invocation,
                }));
                if self.send(request) {
                    self.actions.form.status = "Requesting action from owner…".into();
                }
            },
            Err(reason) => self.actions.form.status = reason,
        }
    }
}

pub(super) fn view(state: &State) -> GraphView {
    let busy = state.requested != state.completed;
    let mut children: Vec<GraphView> = vec![Box::new(el("h2", "Record actions"))];
    if let Some((key, card)) = state.focused.as_ref().zip(state.record.as_ref()) {
        children.push(Box::new(el("h3", card.title.clone())));
        if let Some((target, offered)) = &state.actions.offers {
            for action in offered {
                let (key, target, bound) = (key.clone(), target.clone(), action.clone());
                children.push(Box::new(
                    button(action.label.clone(), move |state: &mut State, _| {
                        state.begin_action(&key, &target, &bound);
                    })
                    .attr("title", action.explanation.clone())
                    .attr("aria-disabled", busy.to_string())
                    .attr("data-owner-action", action.intent.0.clone()),
                ));
            }
        }
    }
    if !state.actions.has_offers() {
        children.push(Box::new(el(
            "p",
            "The owner offers no actions for this record.",
        )));
    }
    if let Some(draft) = &state.actions.form.draft
        && draft.action().input_form.is_some()
    {
        let semantics = draft.semantics();
        for field in semantics.fields {
            let choices: Vec<GraphView> = field
                .choices
                .into_iter()
                .map(|choice| {
                    let (name, value) = (field.name.clone(), choice.value);
                    Box::new(
                        button(choice.label, move |state: &mut State, _| {
                            if state.requested == state.completed
                                && let Some(draft) = state.actions.form.draft.as_mut()
                                && let Err(error) = draft.choose(&name, &value)
                            {
                                state.actions.form.status = error.to_string();
                            }
                        })
                        .attr("aria-disabled", busy.to_string())
                        .attr("aria-pressed", choice.selected.to_string()),
                    ) as GraphView
                })
                .collect();
            children.push(Box::new(el(
                "fieldset",
                (el("legend", field.label), choices),
            )));
        }
        children.push(Box::new(
            button(semantics.submit_label, |state: &mut State, _| {
                state.submit_action();
            })
            .attr("aria-disabled", busy.to_string()),
        ));
        children.push(Box::new(
            button("Cancel", |state: &mut State, _| {
                if state.requested == state.completed {
                    state.actions.form.close();
                    state.actions.form.status.clear();
                }
            })
            .attr("aria-disabled", busy.to_string()),
        ));
    }
    children.push(Box::new(
        el("p", state.actions.form.status.clone()).attr("role", "status"),
    ));
    Box::new(
        el::<_, State, ()>("section", children)
            .attr("class", "resident-actions")
            .attr(
                "style",
                format!("height: {}px", state.size.1.saturating_add(DETAIL_HEIGHT)),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chirograph::{
        ActionFormChoiceV1, ActionFormFieldV1, ActionFormV1, IntentEffect, IntentReference,
        ProjectionSession,
    };
    use sceno::InstanceId;
    use scenotime::{Revision, SceneEpoch};

    #[test]
    fn record_form_keeps_its_owner_values_and_observed_revision() {
        let action = AdvertisedAction {
            intent: IntentReference("fixture.choose".into()),
            label: "Choose".into(),
            explanation: "Choose a disclosed value.".into(),
            payload_schema: "fixture.choose/v1".into(),
            input_form: Some(ActionFormV1::new("fixture.choose/v1").with_field(
                ActionFormFieldV1::choice(
                    "choice",
                    "Choice",
                    [ActionFormChoiceV1::new("owner-value", "Owner value")],
                ),
            )),
            effect: IntentEffect::DomainTruth,
            icon: None,
        };
        let target = ActionDraftTarget {
            session: ProjectionSession("graph".into()),
            target: InstanceId(17),
            observed_epoch: SceneEpoch(3),
            observed_revision: Revision(6),
        };
        let mut panel = Panel {
            offers: Some((target.clone(), vec![action.clone()])),
            ..Default::default()
        };
        let card = PortableCardV1 {
            title: "Record".into(),
            values: Vec::new(),
            badges: Vec::new(),
            media: Vec::new(),
        };
        let mut draft = ActionDraft::new(action.clone());
        assert!(compose(&card, &draft).is_err());
        assert!(draft.choose("choice", "invented").is_err());
        draft.choose("choice", "owner-value").unwrap();
        let command = compose(&card, &draft).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&command.arguments).unwrap();
        assert_eq!(
            payload,
            serde_json::json!({
                "schema": "fixture.choose/v1",
                "choice": "owner-value",
            })
        );
        assert!(panel.still_offered(&target, &action));
        panel.offers.as_mut().unwrap().0.observed_revision = Revision(7);
        assert!(!panel.still_offered(&target, &action));
    }
}
