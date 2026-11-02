// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Lower one assistive action into the app, through the same update spine a
/// keypress uses. Returns the effects for the shell to run.
///
/// A request whose node has no route is a miss a scenario must be able to
/// see, so it lands in the event stream as `interaction-missed a11y-action`
/// rather than vanishing: the loud-and-attributable rule, unchanged from
/// pointer clicks.
pub(crate) fn apply_route(
    app: &mut App,
    route: Option<&A11yRoute>,
    target: accesskit::NodeId,
) -> Vec<crate::action::Effect> {
    match route {
        Some(A11yRoute::OpenOmnibar) => {
            app.update(crate::action::Action::OmnibarOpen { command: false })
        },
        Some(A11yRoute::Action(action)) => app.update(action.clone()),
        Some(A11yRoute::SelectMember { pane, member }) => {
            if app.graph_pane_select_member(*pane, *member) {
                vec![crate::action::Effect::Redraw]
            } else {
                app.note(crate::observe::AppEvent::InteractionMissed {
                    what: "a11y-action",
                    target: member.to_string(),
                });
                Vec::new()
            }
        },
        Some(A11yRoute::Contributed { .. }) | Some(A11yRoute::Frame(_)) => {
            // The shell owns retained sessions and intercepts this variant
            // before app-only route lowering. Keep a misplaced call loud.
            app.note(crate::observe::AppEvent::InteractionMissed {
                what: "a11y-action",
                target: "contributed route reached app-only lowering".to_string(),
            });
            Vec::new()
        },
        None => {
            app.note(crate::observe::AppEvent::InteractionMissed {
                what: "a11y-action",
                target: format!("{target:?}"),
            });
            Vec::new()
        },
    }
}
