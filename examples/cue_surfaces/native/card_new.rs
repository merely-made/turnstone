// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

impl Card {
    pub(super) fn new(content: PeltSharedContent) -> Self {
        Self {
            content,
            presenter: Presenter::default(),
            events: vec![],
            last_sequence: 0,
            requests: 0,
            completions: vec![],
            hash: None,
            stamp: None,
            size: None,
            background: None,
            typed_observed: false,
            navigation_observed: false,
            navigation_observations: 0,
            retiring: false,
            pending_readback: None,
        }
    }

    pub(super) fn command(&mut self, command: Command) -> R<RequestKey> {
        self.requests += 1;
        self.content.request(RequestId(self.requests), command)
    }

    pub(super) fn drain(&mut self) -> R<()> {
        while let Some(e) = self.content.poll_event() {
            if e.scope != self.content.scope() || e.sequence <= self.last_sequence {
                return Err(Error::Invalid("event scope/order changed".into()));
            }
            self.last_sequence = e.sequence;
            if let Event::Completed {
                result: Err(error), ..
            } = &e.event
            {
                if !self.retiring || !matches!(error, Error::Retired) {
                    return Err(Error::Backend(format!("accepted command failed: {error}")));
                }
            }
            if matches!(
                &e.event,
                Event::NavigationRequested { .. } | Event::NavigationFinished { success: true, .. }
            ) {
                self.navigation_observations += 1;
            }
            match &e.event {
                Event::NavigationRequested { address }
                | Event::NavigationStarted { address }
                | Event::AddressChanged(address)
                    if address.ends_with("page-b.html") =>
                {
                    self.navigation_observed = true
                },
                Event::TitleChanged(title)
                    if title.starts_with("Cue value=") && title.contains('X') =>
                {
                    self.typed_observed = true
                },
                Event::Semantics {
                    payload: SemanticPayload::Snapshot(projection),
                    ..
                } if projection
                    .nodes()
                    .iter()
                    .any(|node| node.value.as_deref().is_some_and(|v| v.contains('X'))) =>
                {
                    self.typed_observed = true
                },
                _ => {},
            }
            if let Event::Completed { request, .. } = &e.event {
                if self.completions.contains(request) {
                    return Err(Error::Invalid("duplicate terminal result".into()));
                }
                self.completions.push(*request);
            }
            self.events.push(serde_json::json!({"sequence":e.sequence,"scope":format!("{:?}",e.scope),"event":format!("{:?}",e.event)}));
        }
        Ok(())
    }

    pub(super) fn input(&mut self, input: SessionInput) -> R<()> {
        self.content.input(Input::Document(input))
    }

    pub(super) fn edit(&mut self) -> R<()> {
        self.input(SessionInput::Focus(true))?;
        self.input(SessionInput::PointerMoved {
            x: 40.,
            y: 35.,
            modifiers: Default::default(),
        })?;
        for state in [SessionButtonState::Pressed, SessionButtonState::Released] {
            self.input(SessionInput::PointerButton {
                x: 40.,
                y: 35.,
                button: SessionPointerButton::Primary,
                state,
                modifiers: Default::default(),
            })?;
        }
        self.input(SessionInput::Text("X".into()))?;
        self.command(Command::Zoom(1.25))?;
        if self
            .content
            .capabilities()
            .feature(Feature::Semantics)
            .permits()
        {
            self.command(Command::SetSemantics(true))?;
        }
        Ok(())
    }

    pub(super) fn retire(&mut self) -> R<()> {
        let key = self.command(Command::Zoom(1.))?;
        self.content.retire()?;
        self.retiring = true;
        self.drain()?;
        if self.completions.iter().filter(|x| **x == key).count() != 1 {
            return Err(Error::Invalid("retirement lost an accepted request".into()));
        }
        if !matches!(self.input(SessionInput::Focus(true)), Err(Error::Retired)) {
            return Err(Error::Invalid("retired receiver accepted input".into()));
        }
        self.presenter.retire();
        Ok(())
    }
}
