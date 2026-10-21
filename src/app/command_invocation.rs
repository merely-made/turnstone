// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Turnstone owns its command observation, decoding, reducer and completion.

use super::{App, palette::CONTEXT_COMMANDS};
use crate::action::{Action, Effect};
use cambium::{Invocation, Outcome};

const SCHEMA: &str = "turnstone.command/v1";

#[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Observation {
    session: crate::panes::SessionId,
    pane: Option<crate::panes::PaneId>,
    graph: Option<crate::panes::GraphId>,
    member: Option<uuid::Uuid>,
}

impl App {
    fn command_observation(&self) -> Observation {
        let pane = self.scene_command_pane();
        Observation {
            session: self.session_id.clone(),
            pane,
            graph: pane.and_then(|pane| self.graph_for_pane(pane)),
            member: pane.and_then(|pane| self.graph_pane_focused_member(pane)),
        }
    }

    /// Bind a declared static command to the current host observation. Text
    /// editing and contextual producer actions retain their dedicated paths.
    pub(crate) fn command_invocation(&self, action: &Action) -> Option<Invocation> {
        let catalog = self.command_catalog();
        let command = catalog.set.commands().iter().find(|command| {
            command.category != CONTEXT_COMMANDS && catalog.action(&command.id) == Some(action)
        })?;
        Some(
            command.invocation(
                SCHEMA,
                serde_json::to_vec(&self.command_observation())
                    .expect("command observation is serializable"),
            ),
        )
    }

    /// Recheck an invocation at the reducer boundary. A missing terminal
    /// outcome means deferred or not yet established, never inferred success.
    pub(crate) fn invoke_command(
        &mut self,
        invocation: &Invocation,
    ) -> (Vec<Effect>, Option<Outcome>) {
        let refuse = |reason: &str| {
            (
                Vec::new(),
                Some(Outcome::Refused {
                    reason: reason.into(),
                }),
            )
        };
        if invocation.schema_id != SCHEMA {
            return refuse("unsupported command argument schema");
        }
        let Ok(observed) = serde_json::from_slice::<Observation>(&invocation.arguments) else {
            return refuse("invalid command observation");
        };
        if observed != self.command_observation() {
            return (Vec::new(), Some(Outcome::Stale));
        }
        let catalog = self.command_catalog();
        let Some(command) = catalog.set.get(&invocation.id) else {
            return (Vec::new(), Some(Outcome::Stale));
        };
        if command.category == CONTEXT_COMMANDS || command.effect != invocation.effect {
            return refuse("command does not match the current declaration");
        }
        if let Some(reason) = &command.disabled_reason {
            return refuse(reason);
        }
        let action = catalog
            .action(&invocation.id)
            .expect("registered command has an action")
            .clone();
        if self.session_load_refused() && action != Action::NewSession {
            return refuse("session loading was refused");
        }
        // This operation completes synchronously in the view model. Other
        // reducers may emit effects or ask for confirmation; they stay pending.
        let physics = if action == Action::TogglePhysics {
            let Some(pane) = self.scene_command_pane() else {
                return (Vec::new(), Some(Outcome::Stale));
            };
            let Some(graph) = self.graph_for_pane(pane) else {
                return (Vec::new(), Some(Outcome::Stale));
            };
            let Some(canvas) = self.graph_runtimes.canvas(graph) else {
                return (Vec::new(), Some(Outcome::Stale));
            };
            Some((graph, canvas.physics_paused()))
        } else {
            None
        };
        let isometric =
            (action == Action::ToggleIsometric).then(|| self.graph_runtimes.is_isometric());
        let effects = self.update(action);
        let outcome = physics.map(|(graph, before)| {
            if self
                .graph_runtimes
                .canvas(graph)
                .is_some_and(|canvas| canvas.physics_paused() != before)
            {
                Outcome::Done
            } else {
                Outcome::Refused {
                    reason: "physics view did not change".into(),
                }
            }
        });
        let outcome = outcome.or_else(|| {
            isometric.map(|before| {
                if self.graph_runtimes.is_isometric() != before {
                    Outcome::Done
                } else {
                    Outcome::Refused {
                        reason: "isometric view did not change".into(),
                    }
                }
            })
        });
        (effects, outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hest_command_enacts_the_existing_reducer_and_observes_completion() {
        let mut app = App::test_stub();
        let invocation = app.command_invocation(&Action::TogglePhysics).unwrap();
        assert_eq!(invocation.id, cambium::catalogue::ids::PHYSICS_TOGGLE);
        let graph = app.command_observation().graph.unwrap();
        let before = app.graph_runtimes.canvas(graph).unwrap().physics_paused();
        let (effects, outcome) = app.invoke_command(&invocation);
        assert_eq!(outcome, Some(Outcome::Done));
        assert_eq!(effects, vec![Effect::Redraw]);
        assert_eq!(
            app.graph_runtimes.canvas(graph).unwrap().physics_paused(),
            !before
        );
    }

    #[test]
    fn hest_command_rejects_bad_contracts_and_stale_targets_before_enactment() {
        let mut app = App::test_stub();
        let command = app.command_invocation(&Action::TogglePhysics).unwrap();
        for field in 0..3 {
            let mut invalid = command.clone();
            match field {
                0 => invalid.schema_id.push_str("-other"),
                1 => invalid.arguments.clear(),
                _ => invalid.effect = cambium::EffectClass::DomainTruth,
            }
            let (effects, outcome) = app.invoke_command(&invalid);
            assert!(effects.is_empty());
            assert!(matches!(outcome, Some(Outcome::Refused { .. })));
        }
        let mut unknown = command.clone();
        unknown.id = "missing:command".into();
        assert_eq!(app.invoke_command(&unknown), (vec![], Some(Outcome::Stale)));
        app.session_id = crate::panes::SessionId::new();
        assert_eq!(app.invoke_command(&command), (vec![], Some(Outcome::Stale)));
    }

    #[test]
    fn hest_command_does_not_call_a_deferred_save_done() {
        let mut app = App::test_stub();
        let command = app.command_invocation(&Action::SaveSession).unwrap();
        let (effects, outcome) = app.invoke_command(&command);
        assert!(effects.contains(&Effect::SaveSession));
        assert_eq!(outcome, None);
    }
}
