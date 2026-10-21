// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! The selected record's current owner offers, using existing forms/commands.
use super::*;
use cambium::button;
use chirograph::AdvertisedAction;
use graphshell::handlers::{OPEN_ADDRESS_SCHEMA, OpenAddressV1, handler_from_intent};
use graphshell_client::ActionDraftTarget;

pub(super) type OwnerOffers = HashMap<String, (ActionDraftTarget, Vec<AdvertisedAction>)>;

/// Address handoff already has a typed payload; bounded forms use the owner's
/// exact fields/values. This does not invent a generic editor for other schemas.
fn command(
    card: &PortableCardV1,
    draft: &graphshell_client::ActionDraft,
) -> Result<hest::Invocation, String> {
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

impl App {
    pub(super) fn open_owner_action(&mut self, intent: &str) {
        if self.pending || self.coop.as_ref().is_some_and(|panel| panel.guest) {
            return;
        }
        let Some(key) = self.selected.clone() else {
            return;
        };
        let Some((target, actions)) = self.owner_offers.get(&key) else {
            return;
        };
        let Some(action) = actions
            .iter()
            .find(|action| action.intent.0 == intent)
            .cloned()
        else {
            return;
        };
        self.owner_record = Some(key);
        let bounded = self.owner_form.open_action(
            target.session.clone(),
            target.target,
            action,
            (target.observed_epoch, target.observed_revision),
        );
        if !bounded {
            self.submit_owner_action();
        }
    }

    pub(super) fn submit_owner_action(&mut self) {
        if self.pending || self.coop.as_ref().is_some_and(|panel| panel.guest) {
            return;
        }
        let result = (|| {
            let key = self.owner_record.as_ref().ok_or("Select a record first.")?;
            let target = self
                .owner_form
                .target
                .as_ref()
                .ok_or("Open an owner action first.")?;
            let draft = self
                .owner_form
                .draft
                .as_ref()
                .ok_or("Open an owner action first.")?;
            let current = self
                .owner_offers
                .get(key)
                .ok_or("Owner actions are unavailable.")?;
            if self.selected.as_ref() != Some(key)
                || &current.0 != target
                || !current.1.contains(draft.action())
            {
                return Err(
                    "The selected record or its owner actions changed; reopen the action.".into(),
                );
            }
            let card = self
                .cards
                .get(key)
                .ok_or("The record is no longer disclosed.")?;
            Ok(WorkerRequest::Owner {
                key: key.clone(),
                target: target.clone(),
                action: draft.action().clone(),
                command: command(card, draft)?,
            })
        })();
        match result {
            Ok(request) => {
                if self.sender.send(request).is_ok() {
                    self.pending = true;
                    self.owner_waiting = true;
                    self.owner_form.status = "Requesting action from owner…".into();
                } else {
                    self.owner_form.status = "The resident worker stopped.".into();
                }
            },
            Err(reason) => self.owner_form.status = reason,
        }
    }
}

pub(super) fn view(app: &App) -> Child {
    let mut children: Vec<Child> = Vec::new();
    if let Some(key) = &app.selected {
        match app.owner_offers.get(key) {
            Some((_, actions)) if !actions.is_empty() => {
                for action in actions {
                    let intent = action.intent.0.clone();
                    children.push(Box::new(
                        button(action.label.clone(), move |app: &mut App, _| {
                            app.open_owner_action(&intent)
                        })
                        .attr("title", action.explanation.clone())
                        .attr("aria-disabled", app.pending.to_string())
                        .attr("data-owner-action", action.intent.0.clone()),
                    ));
                }
            },
            Some(_) => children.push(Box::new(el(
                "p",
                "No actions offered by this record’s owner.",
            ))),
            None => children.push(Box::new(el("p", "Owner actions unavailable."))),
        }
        if app.owner_record.as_ref() == Some(key) {
            if let Some(draft) = &app.owner_form.draft {
                let semantics = draft.semantics();
                for field in semantics.fields {
                    let name = field.name;
                    let choices: Vec<Child> = field
                        .choices
                        .into_iter()
                        .map(|choice| {
                            let (name, value) = (name.clone(), choice.value);
                            Box::new(
                                button(choice.label, move |app: &mut App, _| {
                                    if !app.pending
                                        && let Some(draft) = app.owner_form.draft.as_mut()
                                    {
                                        if let Err(error) = draft.choose(&name, &value) {
                                            app.owner_form.status = error.to_string();
                                        }
                                    }
                                })
                                .attr("aria-disabled", app.pending.to_string())
                                .attr("aria-pressed", choice.selected.to_string()),
                            ) as Child
                        })
                        .collect();
                    children.push(Box::new(el(
                        "fieldset",
                        (el("legend", field.label), choices),
                    )));
                }
                if draft.action().input_form.is_some() {
                    children.push(Box::new(
                        button(semantics.submit_label, |app: &mut App, _| {
                            app.submit_owner_action()
                        })
                        .attr("data-owner-submit", "true"),
                    ));
                    children.push(Box::new(button("Cancel", |app: &mut App, _| {
                        if !app.pending {
                            app.owner_form.close();
                            app.owner_record = None;
                        }
                    })));
                }
            }
            children.push(Box::new(
                el("p", app.owner_form.status.clone()).attr("role", "status"),
            ));
        }
    }
    Box::new(
        el("div", children)
            .attr("role", "group")
            .attr("class", "mere-owner-actions")
            .attr("aria-label", "Owner actions"),
    )
}
