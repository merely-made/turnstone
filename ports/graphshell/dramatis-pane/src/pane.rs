// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The pane's state: the mounted projection, the resolved cards and the one
//! open action form.
use chirograph::{
    AdvertisedAction, CapabilityProfile, IntentResult, PortableCardV1, PresentationCapability,
    ProjectionSession, Revision, SceneEpoch,
};
use dramatis_surface::{Custody, DramatisSurface};
use graphshell_client::{ActionForm, ClientState, PresentationResolution, ResolvedContent};
use graphshell_endpoint::{IntentSink, PresentationSource, ProjectionSource};
use sceno::InstanceId;

/// One resolved card and what the surface advertises on it.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneCard {
    pub instance: InstanceId,
    pub card: PortableCardV1,
    pub actions: Vec<AdvertisedAction>,
}

/// A host's mounted Dramatis surface.
pub struct Pane<C: Custody> {
    surface: DramatisSurface<C>,
    client: ClientState,
    session: ProjectionSession,
    observed: Option<(SceneEpoch, Revision)>,
    cards: Vec<PaneCard>,
    /// The open bounded form, if an action needs a choice before it is sent.
    pub form: ActionForm,
    /// The instance the open form belongs to.
    pub form_card: Option<InstanceId>,
    /// Why the last action was refused, in words; empty when nothing was.
    pub message: String,
}

impl<C: Custody> Pane<C> {
    pub fn new(custody: C) -> Self {
        let surface = DramatisSurface::new(custody);
        let session = surface.session();
        let mut pane = Self {
            surface,
            client: ClientState::default(),
            session,
            observed: None,
            cards: Vec::new(),
            form: ActionForm::new(""),
            form_card: None,
            message: String::new(),
        };
        pane.refresh();
        pane
    }

    pub fn cards(&self) -> &[PaneCard] {
        &self.cards
    }

    /// The surface, for hosts and tests that need to read its custody state.
    pub fn surface(&self) -> &DramatisSurface<C> {
        &self.surface
    }

    /// Whether Djinn has a request outstanding. Controls stay but are disabled.
    pub fn busy(&self) -> bool {
        self.cards
            .iter()
            .any(|c| c.card.badges.iter().any(|b| b == "busy"))
    }

    /// Read the surface again and resolve every card. Call on the worker's
    /// wake and after any action.
    pub fn refresh(&mut self) {
        let request = self.surface.request();
        let snapshot = match self.surface.snapshot(request) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.message = error.to_string();
                return;
            },
        };
        self.observed = Some((snapshot.scene.epoch, snapshot.scene.revision));
        if let Err(error) = self.client.apply_snapshot(snapshot) {
            self.message = format!("Dramatis sent a surface this host cannot show: {error:?}");
            self.cards.clear();
            return;
        }
        let profile = CapabilityProfile::new([PresentationCapability::PortableCard]);
        let Ok(tree) = self.client.accessibility_tree(&self.session, &profile) else {
            self.cards.clear();
            return;
        };
        let mut cards = Vec::new();
        for item in tree.children {
            let mut resolved = self.client.resolve(&self.session, item.instance, &profile);
            if let Ok(PresentationResolution::NeedsResource(request)) = &resolved {
                if let Ok(response) = self.surface.resource(request.clone()) {
                    let _ = self.client.apply_resource(response);
                    resolved = self.client.resolve(&self.session, item.instance, &profile);
                }
            }
            if let Ok(PresentationResolution::Ready(ready)) = resolved {
                if let ResolvedContent::PortableCard(card) = ready.content {
                    cards.push(PaneCard {
                        instance: item.instance,
                        card,
                        actions: item.actions,
                    });
                }
            }
        }
        self.cards = cards;
        // A form opened against a snapshot that has moved on is not sent.
        if let (Some(open), Some(card)) = (&self.form.target, self.form_card) {
            let current = self.observed;
            if current != Some((open.observed_epoch, open.observed_revision))
                || !self.cards.iter().any(|c| c.instance == card)
            {
                self.close_form();
                self.message =
                    "Dramatis changed while the form was open. Choose the action again.".into();
            }
        }
    }

    /// A person activated `intent` on `instance`. A plain action is sent now;
    /// one with a bounded form opens it and waits for the choice.
    pub fn activate(&mut self, instance: InstanceId, intent: &str) {
        if self.busy() {
            self.message = "Djinn is waiting for you. Finish its prompt first.".into();
            return;
        }
        let Some((card, action)) =
            self.cards
                .iter()
                .find(|c| c.instance == instance)
                .and_then(|c| {
                    c.actions
                        .iter()
                        .find(|a| a.intent.0 == intent)
                        .map(|a| (c.clone(), a.clone()))
                })
        else {
            self.message = "That action is no longer offered.".into();
            return;
        };
        let Some(observed) = self.observed else {
            return;
        };
        self.message.clear();
        let bounded = self
            .form
            .open_action(self.session.clone(), card.instance, action, observed);
        if bounded {
            self.form_card = Some(card.instance);
        } else {
            self.form_card = Some(card.instance);
            self.submit();
        }
    }

    /// Choose `value` for the open form's field `name`.
    pub fn choose(&mut self, name: &str, value: &str) {
        if let Some(draft) = self.form.draft.as_mut() {
            if let Err(error) = draft.choose(name, value) {
                self.form.status = error.to_string();
            }
        }
    }

    pub fn close_form(&mut self) {
        self.form.close();
        self.form_card = None;
        self.form.status.clear();
    }

    /// Send the open action to the surface.
    pub fn submit(&mut self) {
        let (Some(target), Some(draft)) = (self.form.target.clone(), self.form.draft.as_mut())
        else {
            return;
        };
        let invocation = match draft.invocation(&target) {
            Ok(invocation) => invocation,
            Err(error) => {
                self.form.status = error.to_string();
                return;
            },
        };
        match self.surface.invoke(invocation) {
            Ok(IntentResult::Accepted) => {
                self.message.clear();
                self.close_form();
            },
            Ok(IntentResult::Rejected { reason }) => {
                self.message = reason;
                self.close_form();
            },
            Ok(IntentResult::Stale { .. }) => {
                self.message = "Dramatis changed. Choose the action again.".into();
                self.close_form();
            },
            Err(error) => {
                self.message = error.to_string();
                self.close_form();
            },
        }
        self.refresh();
    }
}
