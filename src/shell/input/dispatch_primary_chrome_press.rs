// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn dispatch_primary_chrome_press(
    host: &mut impl PrimaryPressAdmissionHost,
    intent: Option<crate::chrome_view::ChromeIntent>,
) -> bool {
    let Some(intent) = intent else {
        return false;
    };
    let action = match intent {
        crate::chrome_view::ChromeIntent::CommitRow(index) => Action::OmnibarCommitRow(index),
        crate::chrome_view::ChromeIntent::ToggleKeep(index) => Action::OmnibarToggleKeepRow(index),
        crate::chrome_view::ChromeIntent::Scope(command) => Action::Scope(command),
        crate::chrome_view::ChromeIntent::FocusCard(command) => Action::FocusCard(command),
        crate::chrome_view::ChromeIntent::NavBack => Action::NavBack,
        crate::chrome_view::ChromeIntent::NavForward => Action::NavForward,
        crate::chrome_view::ChromeIntent::Reload => Action::Reload,
        crate::chrome_view::ChromeIntent::Stop => Action::Stop,
        crate::chrome_view::ChromeIntent::KeepNode(member) => Action::KeepNode { member },
        crate::chrome_view::ChromeIntent::Address => Action::OmnibarOpen { command: false },
        crate::chrome_view::ChromeIntent::WorkspaceMap(map) => Action::SetWorkspaceMap(map),
        crate::chrome_view::ChromeIntent::Tool(kind) => {
            Action::ToggleToolPane(crate::panes::PaneKindId::new(kind))
        },
        crate::chrome_view::ChromeIntent::ReadMember(member) => Action::ReadGraphMember(member),
        crate::chrome_view::ChromeIntent::NavigateRelationship(target) => {
            Action::NavigateRelationship(target)
        },
        crate::chrome_view::ChromeIntent::RevealRelationship(target) => {
            Action::RevealRelationship(target)
        },
        crate::chrome_view::ChromeIntent::RevealMember(member) => Action::RevealGraphMember(member),
        crate::chrome_view::ChromeIntent::Layout(layout) => Action::SetChromeLayout(layout),
        crate::chrome_view::ChromeIntent::FitGraph => {
            dispatch_primary_action_press(host, Action::SetWorkspaceMap(true));
            dispatch_primary_action_press(host, Action::FitView);
            return true;
        },
        crate::chrome_view::ChromeIntent::Territory => Action::ToggleTerritory,
        crate::chrome_view::ChromeIntent::AllPanes => Action::OmnibarOpen { command: true },
        crate::chrome_view::ChromeIntent::MenuChanged => {
            host.pointer_redraw();
            return true;
        },
        crate::chrome_view::ChromeIntent::Find(event) => {
            let Some(action) = crate::document_find::find_action(event) else {
                return false;
            };
            action
        },
        crate::chrome_view::ChromeIntent::Permission { request, choice } => {
            Action::ChoosePermission { request, choice }
        },
        crate::chrome_view::ChromeIntent::FocusAuthentication(field) => {
            Action::FocusAuthenticationField(field)
        },
        crate::chrome_view::ChromeIntent::ToggleAuthenticationMemory => {
            Action::ToggleAuthenticationMemory
        },
        crate::chrome_view::ChromeIntent::SubmitAuthentication(request) => {
            Action::SubmitAuthentication { request }
        },
        crate::chrome_view::ChromeIntent::CancelAuthentication(request) => {
            Action::CancelAuthentication { request }
        },
    };
    dispatch_primary_action_press(host, action);
    true
}
