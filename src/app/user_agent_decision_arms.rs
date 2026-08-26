use crate::action::Effect;
use crate::observe::AppEvent;
use crate::user_agent_decision::{
    AuthenticationField, PendingUserAgentDecision, PermissionChoice, UserAgentRequestKey,
};
use crate::web_policy::PermissionRetention;

use super::App;

impl App {
    pub(super) fn choose_permission(
        &mut self,
        request: UserAgentRequestKey,
        choice: PermissionChoice,
    ) -> Vec<Effect> {
        if !matches!(
            self.user_agent_decision.active(),
            Some(PendingUserAgentDecision::Permission {
                request: active,
                ..
            }) if *active == request
        ) || !self.user_agent_decision.begin(request)
        {
            return Vec::new();
        }
        let (answer, retention) = match choice {
            PermissionChoice::AllowOnce => {
                (inker::PermissionAnswer::Grant, PermissionRetention::OneShot)
            }
            PermissionChoice::AlwaysAllow => (
                inker::PermissionAnswer::Grant,
                PermissionRetention::Remember,
            ),
            PermissionChoice::Deny => {
                (inker::PermissionAnswer::Deny, PermissionRetention::Remember)
            }
            PermissionChoice::Dismiss => (
                inker::PermissionAnswer::Dismiss,
                PermissionRetention::OneShot,
            ),
        };
        vec![
            Effect::AnswerPermissionRequest {
                request,
                answer,
                retention,
            },
            Effect::Redraw,
        ]
    }

    pub(super) fn focus_authentication_field(&mut self, field: AuthenticationField) -> Vec<Effect> {
        self.user_agent_decision
            .focus_authentication(field)
            .then_some(Effect::Redraw)
            .into_iter()
            .collect()
    }

    pub(super) fn insert_authentication(&mut self, text: String) -> Vec<Effect> {
        self.user_agent_decision
            .insert_authentication(&text)
            .then_some(Effect::Redraw)
            .into_iter()
            .collect()
    }

    pub(super) fn backspace_authentication(&mut self) -> Vec<Effect> {
        self.user_agent_decision
            .backspace_authentication()
            .then_some(Effect::Redraw)
            .into_iter()
            .collect()
    }

    pub(super) fn toggle_authentication_memory(&mut self) -> Vec<Effect> {
        self.user_agent_decision
            .toggle_remember_for_process()
            .then_some(Effect::Redraw)
            .into_iter()
            .collect()
    }

    pub(super) fn submit_authentication(&mut self, request: UserAgentRequestKey) -> Vec<Effect> {
        if !matches!(
            self.user_agent_decision.active(),
            Some(PendingUserAgentDecision::Authentication {
                request: active,
                ..
            }) if *active == request
        ) || !self.user_agent_decision.begin(request)
        {
            return Vec::new();
        }
        let draft = &self.user_agent_decision.authentication;
        vec![
            Effect::AnswerAuthenticationRequest {
                request,
                credentials: Some((draft.username.clone(), draft.password.clone())),
                remember_for_process: draft.remember_for_process,
            },
            Effect::Redraw,
        ]
    }

    pub(super) fn cancel_authentication(&mut self, request: UserAgentRequestKey) -> Vec<Effect> {
        if !matches!(
            self.user_agent_decision.active(),
            Some(PendingUserAgentDecision::Authentication {
                request: active,
                ..
            }) if *active == request
        ) || !self.user_agent_decision.begin(request)
        {
            return Vec::new();
        }
        vec![
            Effect::AnswerAuthenticationRequest {
                request,
                credentials: None,
                remember_for_process: false,
            },
            Effect::Redraw,
        ]
    }

    pub(super) fn apply_permission_requested(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        origin: String,
        descriptors: Vec<inker::PermissionDescriptor>,
    ) -> Vec<Effect> {
        let request = UserAgentRequestKey::new(node, id);
        let origin = crate::web_policy::canonical_origin(&origin);
        if !self
            .user_agent_decision
            .receive(PendingUserAgentDecision::Permission {
                request,
                origin: origin.clone(),
                descriptors: descriptors.clone(),
            })
        {
            return Vec::new();
        }
        self.events.push(AppEvent::PermissionRequested {
            node,
            id,
            origin,
            descriptors,
        });
        vec![Effect::Redraw]
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_authentication_requested(
        &mut self,
        node: uuid::Uuid,
        id: inker::UserAgentRequestId,
        host: String,
        port: u16,
        realm: Option<String>,
        scheme: String,
        is_proxy: bool,
    ) -> Vec<Effect> {
        let request = UserAgentRequestKey::new(node, id);
        let host = host.to_ascii_lowercase();
        let scheme = scheme.to_ascii_lowercase();
        if !self
            .user_agent_decision
            .receive(PendingUserAgentDecision::Authentication {
                request,
                host: host.clone(),
                port,
                realm: realm.clone(),
                scheme: scheme.clone(),
                is_proxy,
            })
        {
            return Vec::new();
        }
        self.events.push(AppEvent::AuthenticationRequested {
            node,
            id,
            host,
            realm,
            scheme,
        });
        vec![Effect::Redraw]
    }

    pub(super) fn apply_permission_request_finished(
        &mut self,
        request: UserAgentRequestKey,
        answer: inker::PermissionAnswer,
        retention: PermissionRetention,
        terminal: bool,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let succeeded = result.is_ok();
        let error = result
            .is_err()
            .then(|| "Could not apply the permission decision".to_string());
        if !self.user_agent_decision.finish(request, terminal, error) {
            return Vec::new();
        }
        self.events.push(AppEvent::PermissionAnswered {
            node: request.node,
            id: request.id,
            answer,
            remembered: retention == PermissionRetention::Remember,
            succeeded,
        });
        vec![Effect::Redraw]
    }

    pub(super) fn apply_authentication_request_finished(
        &mut self,
        request: UserAgentRequestKey,
        supplied_credentials: bool,
        remember_for_process: bool,
        terminal: bool,
        result: Result<(), String>,
    ) -> Vec<Effect> {
        let succeeded = result.is_ok();
        let error = result
            .is_err()
            .then(|| "Could not answer the authentication request".to_string());
        if !self.user_agent_decision.finish(request, terminal, error) {
            return Vec::new();
        }
        self.events.push(AppEvent::AuthenticationAnswered {
            node: request.node,
            id: request.id,
            supplied_credentials,
            remembered_for_process: remember_for_process,
            succeeded,
        });
        vec![Effect::Redraw]
    }

    pub(super) fn apply_user_agent_request_withdrawn(
        &mut self,
        request: UserAgentRequestKey,
        kind: &'static str,
        reason: &'static str,
    ) -> Vec<Effect> {
        if !self.user_agent_decision.remove(request) {
            return Vec::new();
        }
        self.events.push(AppEvent::UserAgentRequestWithdrawn {
            node: request.node,
            id: request.id,
            kind,
            reason,
        });
        vec![Effect::Redraw]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, Update};

    fn permission_update(node: uuid::Uuid, id: u64, origin: &str) -> crate::action::Update {
        Update::PermissionRequested {
            node,
            id: inker::UserAgentRequestId::new(id),
            origin: origin.into(),
            descriptors: vec![inker::PermissionDescriptor::Geolocation],
        }
    }

    #[test]
    fn permission_choice_is_retained_and_lowers_the_captured_request() {
        let mut app = App::test_stub();
        let node = uuid::Uuid::new_v4();
        app.apply_update(permission_update(node, 7, "https://maps.example/path"));
        app.update(Action::ReseedLayout);

        let request = app
            .user_agent_decision
            .active_request()
            .expect("retained request survived redraw-producing work");
        let effects = app.update(Action::ChoosePermission {
            request,
            choice: PermissionChoice::AlwaysAllow,
        });
        assert!(effects.contains(&Effect::AnswerPermissionRequest {
            request,
            answer: inker::PermissionAnswer::Grant,
            retention: PermissionRetention::Remember,
        }));
        assert_eq!(app.user_agent_decision.submitting, Some(request));
    }

    #[test]
    fn withdrawn_request_cannot_answer_a_later_navigation() {
        let mut app = App::test_stub();
        let node = uuid::Uuid::new_v4();
        app.apply_update(permission_update(node, 9, "https://old.example/"));
        let stale = app.user_agent_decision.active_request().unwrap();
        app.apply_update(Update::UserAgentRequestWithdrawn {
            request: stale,
            kind: "permission",
            reason: "navigation-started",
        });
        app.apply_update(permission_update(node, 10, "https://new.example/"));

        assert!(
            app.update(Action::ChoosePermission {
                request: stale,
                choice: PermissionChoice::AlwaysAllow,
            })
            .is_empty()
        );
        assert_eq!(
            app.user_agent_decision.active_request(),
            Some(UserAgentRequestKey::new(
                node,
                inker::UserAgentRequestId::new(10)
            ))
        );
    }

    #[test]
    fn authentication_secrets_are_redacted_and_absent_from_observation() {
        let mut app = App::test_stub();
        let node = uuid::Uuid::new_v4();
        let id = inker::UserAgentRequestId::new(11);
        app.apply_update(Update::AuthenticationRequested {
            node,
            id,
            host: "secure.example".into(),
            port: 443,
            realm: Some("private".into()),
            scheme: "basic".into(),
            is_proxy: false,
        });
        app.update(Action::InsertAuthentication("private-user".into()));
        app.update(Action::FocusAuthenticationField(
            AuthenticationField::Password,
        ));
        app.update(Action::InsertAuthentication("private-password".into()));
        app.update(Action::ToggleAuthenticationMemory);
        let request = UserAgentRequestKey::new(node, id);
        let effects = app.update(Action::SubmitAuthentication { request });

        let Effect::AnswerAuthenticationRequest {
            request: lowered_request,
            credentials: Some((username, password)),
            remember_for_process,
        } = &effects[0]
        else {
            panic!("authentication did not lower to an exact credential answer");
        };
        assert_eq!(*lowered_request, request);
        assert_eq!(username.as_str(), "private-user");
        assert_eq!(password.as_str(), "private-password");
        assert!(*remember_for_process);
        let debug = format!("{effects:?}");
        assert!(!debug.contains("private-user"));
        assert!(!debug.contains("private-password"));
        assert!(debug.contains("[redacted]"));
        let observation = format!("{:?}", crate::observe::snapshot(&app));
        assert!(!observation.contains("private-user"));
        assert!(!observation.contains("private-password"));
    }
}
