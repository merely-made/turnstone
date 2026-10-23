// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Application-owned chrome and pane structure for the composed AccessKit forest.
//!
//! The app projection is pure over `App`. Its spawn outline remains a fallback
//! for lanes without live source ownership. The shell replaces non-Reader
//! document outlines with controller projections and mounts contributed DOMs
//! through native Mere grafts. Platform publication and scenario observations
//! read the same forest in root/children order. Source support declarations
//! remain intact, including Partial and unavailable semantics.

use crate::panes::PaneContent;
use accesskit::{Node, Role};
use uxtree::{UxTree, node_id_for_path, stitch};

use crate::app::App;
use crate::content::{NodeContent, OutlineFact};

/// Project the whole application into one stitched [`UxTree`]: a window root
/// whose children are the chrome subtree and the frisket pane tree, with the
/// workbench tiling under its pane leaf and a document subtree under the
/// canvas leaf for every node with live content.
pub fn project_app(app: &App) -> UxTree {
    let mut no_contribution = |_| None;
    project_app_capturing(app, &mut None, &mut no_contribution)
}

pub(crate) fn lens_window_label(ordinal: usize) -> String {
    format!("Turnstone lens {ordinal}")
}

/// A lens has its own pane tree and window root, without primary chrome or
/// primary graph/document outlines. The shell adds actual visible producers.
pub(crate) fn project_lens_with_contributions(app:&App,ordinal:usize,mut contributions:std::collections::HashMap<crate::panes::PaneId,UxTree>)->Option<(UxTree,A11yRoutes)> {
    let layout=app.lenses.get(ordinal)?.as_ref()?;
    let mut frozen=None;
    let panes=crate::panes::project_frisket_with(layout,|content,pane|match content {
        PaneContent::Workbench=>app.workbench_for_pane(pane).map(mere::platen::project_tile_layout),
        PaneContent::Orrery=>{
            let mut root=Node::new(Role::Group);
            let count=app.graph_for_pane(pane).and_then(|graph|app.graph_runtimes.canvas(graph)).map(|canvas|canvas.graph().nodes().count()).unwrap_or(0);
            root.set_label(format!("graph canvas, {count} nodes"));
            Some(stitch(&format!("turnstone/lens/{ordinal}/canvas/{}",pane.0),root,Vec::new()))
        },
        PaneContent::Registered(kind) if kind.as_str()==crate::panes::kind::FROZEN_PROJECTION=>{frozen=Some(pane);Some(project_frozen_projection(app))},
        PaneContent::Gloss(_)|PaneContent::Registered(_)=>contributions.remove(&pane),
        _=>None,
    });
    let mut root=Node::new(Role::Window);root.set_label(lens_window_label(ordinal));
    let tree=stitch(&format!("turnstone/lens/{ordinal}"),root,vec![panes]);
    let mut routes=A11yRoutes::new();
    if let Some(pane)=frozen {
        for (_,node) in app.graph_runtimes.graph().nodes() {
            routes.insert(node_id_for_path(&format!("turnstone/frozen-projection/instance/{}",node.id)),A11yRoute::SelectMember{pane,member:node.id});
        }
    }
    Some((tree,routes))
}

fn project_app_capturing(
    app: &App,
    orrery_pane: &mut Option<crate::panes::PaneId>,
    contribution: &mut impl FnMut(crate::panes::PaneId) -> Option<UxTree>,
) -> UxTree {
    let chrome = project_chrome(app);
    // Live documents, in graph order (deterministic), each from its mirrored
    // structural outline. The visible canvas carries them when present;
    // a full-width Workbench carries them while Orrery is an inactive tab.
    // A document without a structural read is announced without children.
    let docs: Vec<UxTree> = app
        .graph_runtimes
        .graph()
        .nodes()
        .filter(|(_, n)| matches!(app.content.get(n.id), Some(NodeContent::Live)))
        .map(|(_, n)| project_live_document(app, n.id, n.url()))
        .collect();
    let mut docs = Some(docs);
    let visible_canvas = app.frisket.outer_workspace().ok().is_some_and(|frame| {
        frame.active_ids().iter().any(|id| matches!(app.pane_content(crate::panes::PaneId(id.0)),
            Some(PaneContent::Orrery)))
    });
    let panes = crate::panes::project_frisket_with(&app.frisket, |content, id| match content {
        PaneContent::Workbench => app.workbench_for_pane(id).map(|workbench| {
            let layout = mere::platen::project_tile_layout(workbench);
            if visible_canvas { return layout; }
            let mut children = vec![layout];
            children.extend(docs.take().unwrap_or_default());
            let mut root = Node::new(Role::Group);
            root.set_label("Workbench documents");
            stitch(&format!("turnstone/workbench/{}/documents", id.0), root, children)
        }),
        PaneContent::Orrery => {
            if orrery_pane.is_none() {
                *orrery_pane = Some(id);
            }
            // The canvas leaf carries the graph summary plus the live
            // documents (their pixels composite over the canvas region).
            let count = app
                .graph_for_pane(id)
                .and_then(|graph| app.graph_runtimes.canvas(graph))
                .map(|canvas| canvas.graph().nodes().count())
                .unwrap_or(0);
            let mut root = Node::new(Role::Group);
            root.set_label(format!("graph canvas, {count} nodes"));
            let subtrees = docs.take().unwrap_or_default();
            Some(stitch("turnstone/canvas", root, subtrees))
        },
        PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION => {
            Some(project_frozen_projection(app))
        },
        // Promoted read-only Diagnostics rows and Arrange refusal status contribute here;
        // active built-in controls retain their separate qualification gate.
        PaneContent::Gloss(_) | PaneContent::Registered(_) => contribution(id),
        _ => None,
    });
    let mut root = Node::new(Role::Window);
    root.set_label("Turnstone");
    stitch("turnstone", root, vec![chrome, panes])
}

/// The projection flattened to "role: label" lines for the observation
/// snapshot (what `assert a11y` matches). Values fold into the line when a
/// node has one (the omnibar's text, a link's target).
pub fn a11y_lines(app: &App) -> Vec<String> {
    tree_lines(&project_app(app))
}

/// Flatten any already-composed tree to the lines scenario `assert a11y`
/// reads. The shell uses this overload so live contributed DOM semantics and
/// the platform adapter are checked from the same projection.
pub fn tree_lines(tree: &UxTree) -> Vec<String> {
    forest_lines(&[tree.to_tree_update(None)])
}

/// Traverse the exact host/guest forest supplied to the adapter. Local IDs
/// remain local to their tree, and sibling order comes from children.
pub(crate) fn forest_lines(forest: &[accesskit::TreeUpdate]) -> Vec<String> {
    use accesskit::TreeId;
    let trees: std::collections::HashMap<_, _> = forest.iter().map(|t| (t.tree_id, t)).collect();
    let Some(root) = trees
        .get(&TreeId::ROOT)
        .and_then(|t| t.tree.as_ref())
        .map(|t| t.root)
    else {
        return Vec::new();
    };
    let mut pending = vec![(TreeId::ROOT, root)];
    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    while let Some((tree, id)) = pending.pop() {
        if !seen.insert((tree, id)) {
            continue;
        }
        let Some(node) = trees
            .get(&tree)
            .and_then(|t| t.nodes.iter().find(|(n, _)| *n == id))
            .map(|(_, n)| n)
        else {
            continue;
        };
        if node.is_hidden() {
            continue;
        }
        let role = format!("{:?}", node.role()).to_lowercase();
        lines.push(match (node.label(), node.value()) {
            (Some(label), Some(value)) => format!("{role}: {label} = {value}"),
            (Some(label), None) => format!("{role}: {label}"),
            (None, Some(value)) => format!("{role} = {value}"),
            (None, None) => role,
        });
        if let Some(guest) = node.tree_id() {
            if let Some(root) = trees
                .get(&guest)
                .and_then(|t| t.tree.as_ref())
                .map(|t| t.root)
            {
                pending.push((guest, root));
            }
        } else {
            pending.extend(node.children().iter().rev().map(|id| (tree, *id)));
        }
    }
    lines
}

/// A single substring, or an ordered subsequence separated by ` >> `.
pub(crate) fn matches_lines(lines: &[String], query: &str) -> bool {
    let mut remaining = lines.iter();
    query
        .split(" >> ")
        .all(|part| !part.is_empty() && remaining.any(|line| line.contains(part)))
}

pub(crate) fn host_input_focus(app: &App) -> Option<accesskit::NodeId> {
    app.omnibar
        .open
        .then(|| node_id_for_path("turnstone/chrome/omnibar"))
}

pub(crate) fn focus_host_input(app: &mut App, target: accesskit::NodeId) -> bool {
    if host_input_focus(app) != Some(target) {
        return false;
    }
    app.focus = crate::surface::FocusTarget::Chrome;
    true
}

/// The Frozen Projection pane's subtree: the same AccessKit tree
/// `graphshell_client::frozen` builds for any host, over the same scene the
/// endpoint discloses. Rebuilt from app truth here rather than read out of the
/// renderer, because the a11y projection works off `&App` and the frozen form
/// is a pure reading of the graph.
fn project_frozen_projection(app: &App) -> UxTree {
    let graph = app.graph_runtimes.graph();
    let scene = crate::remote_projection::disclose_scene(
        graph,
        app.graph_runtimes.focused_key(),
        (248.0, 168.0),
        sceno::Spiral::default(),
    );
    let names = graph
        .nodes()
        .map(|(key, node)| {
            (
                sceno::SourceRef::new(cartography::MERE_GRAPH_ADAPTER, node.id.to_string()),
                graph.node_display_label(key),
            )
        })
        .collect();
    graphshell_client::frozen::FrozenScene::freeze(&scene, "Disclosed projection", &names)
        .to_ux_tree("turnstone/frozen-projection")
}

/// The open omnibar's suggestion rows, each a button that commits it, with a
/// Keep or Drop button beside each command in the `>` lane. Both run through
/// the same actions a click does. The install review projects on its own.
fn omnibar_rows(app: &App) -> Vec<(String, Node, Option<A11yRoute>)> {
    if !app.omnibar.open {
        return Vec::new();
    }
    let keep_states = app
        .omnibar
        .text
        .trim()
        .starts_with('>')
        .then(|| app.command_keep_states());
    let mut rows = Vec::new();
    for (index, suggestion) in app.omnibar.suggestions.iter().enumerate() {
        if crate::chrome_view::is_install_review(suggestion)
            || matches!(
                suggestion,
                crate::ui::Suggestion::Hint(_) | crate::ui::Suggestion::Prompt(_)
            )
        {
            continue;
        }
        let text = crate::chrome_view::row_text(suggestion);
        let path = format!("turnstone/chrome/omnibar/row/{index}");
        let mut row = Node::new(Role::Button);
        row.set_label(text);
        row.set_selected(index == app.omnibar.selected);
        rows.push((
            path.clone(),
            row,
            Some(A11yRoute::Action(crate::action::Action::OmnibarCommitRow(
                index,
            ))),
        ));
        if let crate::ui::Suggestion::Act { label, action } = suggestion
            && *action != crate::action::Action::OmnibarShowAllCommands
            && let Some(kept) = keep_states.as_ref().and_then(|states| states.of(label))
        {
            let mut toggle = Node::new(Role::Button);
            toggle.set_label(format!("{} {label}", if kept { "Drop" } else { "Keep" }));
            rows.push((
                format!("{path}/keep"),
                toggle,
                Some(A11yRoute::Action(
                    crate::action::Action::OmnibarToggleKeepRow(index),
                )),
            ));
        }
    }
    rows
}

/// The chrome subtree: the omnibar (a text input when open, with its live
/// text and caret-free honesty) and the at-rest caption.
fn project_chrome(app: &App) -> UxTree {
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    for (path,label,_) in toolbar_actions(app) {
        let id=node_id_for_path(&path);
        let mut node=Node::new(Role::Button);
        node.set_label(label);
        node.add_action(accesskit::Action::Click);
        nodes.push((id,node)); children.push(id);
    }
    if app.omnibar.open {
        let id = node_id_for_path("turnstone/chrome/omnibar");
        let mut n = Node::new(if app.omnibar.sensitive() {
            Role::PasswordInput
        } else {
            Role::TextInput
        });
        let label = match &app.omnibar.mode {
            crate::ui::OmnibarMode::MicronForm(prompt) => match prompt.stage {
                crate::ui::MicronFormStage::Field(index) => prompt
                    .form
                    .fields()
                    .get(index)
                    .map(|f| f.name())
                    .unwrap_or("Micron form"),
                crate::ui::MicronFormStage::Action => "Micron submit action",
                crate::ui::MicronFormStage::Confirm => "Micron submission confirmation",
            },
            _ => "omnibar",
        };
        n.set_label(label);
        n.add_action(accesskit::Action::Focus);
        n.set_value(app.omnibar.presented_text());
        nodes.push((id, n));
        children.push(id);
        for (index, suggestion) in app.omnibar.suggestions.iter().enumerate() {
            if matches!(
                suggestion,
                crate::ui::Suggestion::Prompt(_) | crate::ui::Suggestion::Hint(_)
            ) {
                let id = node_id_for_path(&format!("turnstone/chrome/prompt/{index}"));
                let mut prompt = Node::new(Role::Status);
                prompt.set_label(crate::chrome_view::row_text(suggestion));
                nodes.push((id, prompt));
                children.push(id);
            }
        }
        for (path, node, _) in omnibar_rows(app) {
            let id = node_id_for_path(&path);
            nodes.push((id, node));
            children.push(id);
        }
        if let Some(review) = app
            .omnibar
            .suggestions
            .iter()
            .find(|suggestion| crate::chrome_view::is_install_review(suggestion))
        {
            let id = node_id_for_path("turnstone/chrome/install-review");
            let mut n = Node::new(Role::Button);
            n.set_label(crate::chrome_view::row_text(review));
            nodes.push((id, n));
            children.push(id);
        }
    }
    if app.document_find.open {
        let input_id = node_id_for_path("turnstone/chrome/document-find/input");
        let mut input = Node::new(Role::SearchInput);
        input.set_label("Find in document");
        input.set_value(app.document_find.query());
        nodes.push((input_id, input));
        children.push(input_id);

        let status_id = node_id_for_path("turnstone/chrome/document-find/status");
        let mut status = Node::new(Role::Status);
        status.set_label(app.document_find.status());
        nodes.push((status_id, status));
        children.push(status_id);
    }
    if let Some(decision) = app.user_agent_decision.active() {
        let base = "turnstone/chrome/user-agent-decision";
        let mut decision_children = Vec::new();
        match decision {
            crate::user_agent_decision::PendingUserAgentDecision::Permission { .. } => {
                for (suffix, label) in [
                    ("allow-once", "Allow once"),
                    ("always-allow", "Always allow"),
                    ("deny", "Deny"),
                ] {
                    let id = node_id_for_path(&format!("{base}/{suffix}"));
                    let mut button = Node::new(Role::Button);
                    button.set_label(label);
                    nodes.push((id, button));
                    decision_children.push(id);
                }
            },
            crate::user_agent_decision::PendingUserAgentDecision::Authentication { .. } => {
                let username_id = node_id_for_path(&format!("{base}/username"));
                let mut username = Node::new(Role::TextInput);
                username.set_label("Username");
                username.set_description("credential value withheld from observation");
                nodes.push((username_id, username));
                decision_children.push(username_id);

                let password_id = node_id_for_path(&format!("{base}/password"));
                let mut password = Node::new(Role::PasswordInput);
                password.set_label("Password");
                password.set_description("credential value withheld from observation");
                nodes.push((password_id, password));
                decision_children.push(password_id);

                let remember_id = node_id_for_path(&format!("{base}/remember"));
                let mut remember = Node::new(Role::CheckBox);
                remember.set_label(
                    if app.user_agent_decision.authentication.remember_for_process {
                        "Remember until Turnstone closes, on"
                    } else {
                        "Remember until Turnstone closes, off"
                    },
                );
                nodes.push((remember_id, remember));
                decision_children.push(remember_id);

                for (suffix, label) in [("sign-in", "Sign in"), ("cancel", "Cancel")] {
                    let id = node_id_for_path(&format!("{base}/{suffix}"));
                    let mut button = Node::new(Role::Button);
                    button.set_label(label);
                    nodes.push((id, button));
                    decision_children.push(id);
                }
            },
        }
        if let Some(error) = &app.user_agent_decision.error {
            let error_id = node_id_for_path(&format!("{base}/error"));
            let mut status = Node::new(Role::Status);
            status.set_label(error.clone());
            nodes.push((error_id, status));
            decision_children.push(error_id);
        }
        let id = node_id_for_path(base);
        let mut dialog = Node::new(Role::Dialog);
        dialog.set_label(decision.prompt());
        dialog.set_description(format!(
            "request {} of {}",
            1,
            app.user_agent_decision.len()
        ));
        dialog.set_children(decision_children);
        nodes.push((id, dialog));
        children.push(id);
    }
    if app.shell_chrome_config().projects_shellbar()
        && let Some(caption) = crate::app::navigation_caption(app)
    {
        let id = node_id_for_path("turnstone/chrome/caption");
        let mut n = Node::new(Role::Label);
        n.set_label(caption);
        nodes.push((id, n));
        children.push(id);
    }
    let root_id = node_id_for_path("turnstone/chrome");
    let mut root = Node::new(Role::Group);
    root.set_label("chrome");
    root.set_children(children);
    nodes.push((root_id, root));
    UxTree {
        root: root_id,
        nodes,
    }
}

/// The same semantic view and tool verbs remain available to assistive input
/// without depending on whether the responsive toolbar currently has room.
fn toolbar_actions(app: &App) -> Vec<(String,String,crate::action::Action)> {
    use crate::action::Action;
    if !app.shell_chrome_config().projects_shellbar() { return Vec::new(); }
    let mut actions=vec![
        ("turnstone/chrome/read".into(),"Read document".into(),Action::SetWorkspaceMap(false)),
        ("turnstone/chrome/map".into(),"Reveal document in map".into(),Action::SetWorkspaceMap(true)),
    ];
    for (_,kind) in crate::panes::pane_palette_entries() {
        if matches!(kind.as_str(),crate::panes::kind::GRAPH | crate::panes::kind::WORKBENCH) { continue; }
        if let Some(definition)=crate::panes::pane_definition(kind.as_str()) {
            actions.push((format!("turnstone/chrome/panes/{}",definition.id),
                format!("Toggle {} pane",definition.display_name),Action::ToggleToolPane(kind)));
        }
    }
    actions
}

/// One live document's subtree, from the mirrored structural outline. The
/// root announces as a document and DECLARES the partial capability; outline
/// entries become flat children with mapped roles (depth is structural
/// bookkeeping — nesting reconstruction is a follow-on, said in the
/// description rather than faked).
fn project_live_document(app: &App, node: uuid::Uuid, url: &str) -> UxTree {
    let root_path = format!("turnstone/doc/{node}");
    let root_id = node_id_for_path(&root_path);
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    let structure = app.content.facts(node).and_then(|f| f.structure.as_ref());
    if let Some(s) = structure {
        for (i, entry) in s.outline.iter().enumerate() {
            let id = node_id_for_path(&format!("{root_path}/outline/{i}"));
            let mut n = Node::new(outline_role(entry));
            if !entry.name.is_empty() {
                n.set_label(entry.name.clone());
            }
            nodes.push((id, n));
            children.push(id);
        }
    }
    let mut root = Node::new(Role::Document);
    match structure.and_then(|s| s.title.as_deref()) {
        Some(title) => root.set_label(title.to_string()),
        None => root.set_label(url.to_string()),
    }
    // The honest capability declaration, on the node itself.
    root.set_description(match structure {
        Some(_) => "structural outline only: no bounds, no per-element focus",
        None => "no structural read for this lane",
    });
    root.set_children(children);
    nodes.push((root_id, root));
    UxTree {
        root: root_id,
        nodes,
    }
}

/// Map the outline's coarse role strings (genet's `role_of`) onto AccessKit
/// roles. Unknown strings stay groups.
fn outline_role(entry: &OutlineFact) -> Role {
    match entry.role {
        "link" => Role::Link,
        "button" => Role::Button,
        "textbox" => Role::TextInput,
        "paragraph" => Role::Paragraph,
        "heading" => Role::Heading,
        "list" => Role::List,
        "listitem" => Role::ListItem,
        "image" => Role::Image,
        "label" => Role::Label,
        "navigation" => Role::Navigation,
        "banner" => Role::Banner,
        "contentinfo" => Role::ContentInfo,
        "main" => Role::Main,
        "region" => Role::Section,
        _ => Role::Group,
    }
}

/// Where a screen-reader action on a projected node lands in the app.
///
/// The projection hashes paths into `NodeId`s, which is one-way, so routing
/// is a table built beside the tree rather than parsed back out of it. The
/// meerkat-era checklist called this the route table; this is that idea,
/// rebuilt where the projection actually lives now.
#[derive(Clone, Debug, PartialEq)]
pub enum A11yRoute {
    /// Exact shared frame control; the publishing window owns its route table.
    Frame(crate::frame_controls::FrameRoute),
    /// The chrome: open the omnibar in its find lane.
    OpenOmnibar,
    /// A chrome control: run this action through the update spine, exactly
    /// as its click does (a palette row's commit, its Keep or Drop).
    Action(crate::action::Action),
    /// A frozen-projection instance: select that member in the graph pane, so
    /// a reader who found a node by name can make it the app's selection.
    SelectMember {
        pane: crate::panes::PaneId,
        member: uuid::Uuid,
    },
    /// A retained product surface: route the original action, unchanged, to
    /// the DOM node that Genet projected for this pane.
    Contributed {
        pane: crate::panes::PaneId,
        generation: u64,
        node: genet_scripted_dom::NodeId,
    },
}

/// Routes for every node an assistive action may target.
pub type A11yRoutes = std::collections::HashMap<accesskit::NodeId, A11yRoute>;

/// The projection and its route table, built in one pass.
///
/// Routes may name nodes the current tree does not contain (the frozen pane
/// closed, say); that is harmless, because the platform can only request
/// actions on nodes it was served. The reverse would not be harmless, which
/// is what the coverage test below pins.
pub fn project_app_with_routes(app: &App) -> (UxTree, A11yRoutes) {
    project_app_with_routes_and_contributions(app, std::collections::HashMap::new())
}

pub(crate) fn project_app_with_routes_and_contributions(
    app: &App,
    mut contributions: std::collections::HashMap<crate::panes::PaneId, UxTree>,
) -> (UxTree, A11yRoutes) {
    let mut orrery_pane = None;
    let tree = project_app_capturing(app, &mut orrery_pane, &mut |pane| {
        contributions.remove(&pane)
    });

    let mut routes = A11yRoutes::new();
    for (path,_,action) in toolbar_actions(app) {
        routes.insert(node_id_for_path(&path),A11yRoute::Action(action));
    }
    routes.insert(
        node_id_for_path("turnstone/chrome/omnibar"),
        A11yRoute::OpenOmnibar,
    );
    for (path, _, route) in omnibar_rows(app) {
        if let Some(route) = route {
            routes.insert(node_id_for_path(&path), route);
        }
    }
    if let Some(pane) = orrery_pane {
        for (_, node) in app.graph_runtimes.graph().nodes() {
            routes.insert(
                node_id_for_path(&format!("turnstone/frozen-projection/instance/{}", node.id)),
                A11yRoute::SelectMember {
                    pane,
                    member: node.id,
                },
            );
        }
    }
    (tree, routes)
}

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

#[cfg(test)]
mod route_table {
    use super::*;

    #[test]
    fn every_graph_member_is_reachable_by_route_when_an_orrery_exists() {
        let app = App::projection_fixture();
        let (_, routes) = project_app_with_routes(&app);
        let members: Vec<_> = app
            .graph_runtimes
            .graph()
            .nodes()
            .map(|(_, node)| node.id)
            .collect();
        assert!(!members.is_empty(), "the stub graph has members");
        for member in members {
            let id = node_id_for_path(&format!("turnstone/frozen-projection/instance/{member}"));
            match routes.get(&id) {
                Some(A11yRoute::SelectMember { member: routed, .. }) => {
                    assert_eq!(*routed, member)
                },
                other => panic!("member {member} routed to {other:?}"),
            }
        }
        assert!(matches!(
            routes.get(&node_id_for_path("turnstone/chrome/omnibar")),
            Some(A11yRoute::OpenOmnibar)
        ));
    }

    #[test]
    fn a_routed_selection_changes_the_app_and_a_miss_says_so() {
        let mut app = App::projection_fixture();
        let (_, routes) = project_app_with_routes(&app);
        let member = app
            .graph_runtimes
            .graph()
            .nodes()
            .map(|(_, node)| node.id)
            .next()
            .expect("a member");
        let id = node_id_for_path(&format!("turnstone/frozen-projection/instance/{member}"));

        let effects = apply_route(&mut app, routes.get(&id), id);
        assert!(!effects.is_empty(), "a landed selection redraws");

        // The miss is loud and attributable, exactly like a pointer miss.
        let ghost = node_id_for_path("turnstone/frozen-projection/instance/ghost");
        let effects = apply_route(&mut app, None, ghost);
        assert!(effects.is_empty());
        let described: Vec<String> = app
            .take_events()
            .iter()
            .map(crate::observe::AppEvent::describe)
            .collect();
        assert!(
            described
                .iter()
                .any(|line| line.starts_with("interaction-missed a11y-action")),
            "the miss vanished: {described:?}"
        );
    }

    #[test]
    fn an_omnibar_route_opens_the_omnibar_through_the_spine() {
        let mut app = App::test_stub();
        assert!(!app.omnibar.open);
        apply_route(
            &mut app,
            Some(&A11yRoute::OpenOmnibar),
            node_id_for_path("turnstone/chrome/omnibar"),
        );
        assert!(app.omnibar.open, "the same spine a keypress uses");
    }
}

#[cfg(test)]
mod bridge_integrity {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn a_contributed_tree_attaches_below_its_registered_pane() {
        let mut app = App::test_stub();
        app.update(crate::action::Action::SummonPane(
            crate::panes::PaneKindId::new(crate::panes::kind::SETTINGS),
        ));
        let pane = app
            .frisket
            .iter_leaves()
            .find_map(|(pane, content, _)| match content {
                PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::SETTINGS => {
                    Some(pane)
                },
                _ => None,
            })
            .expect("registered pane");
        let contributed_root = node_id_for_path("test/contributed/root");
        let contributed_child = node_id_for_path("test/contributed/child");
        let mut root = Node::new(Role::Group);
        root.set_label("settings surface");
        root.set_children(vec![contributed_child]);
        let mut child = Node::new(Role::Button);
        child.set_label("Apply");
        let contribution = UxTree {
            root: contributed_root,
            nodes: vec![(contributed_child, child), (contributed_root, root)],
        };

        let (tree, _) = project_app_with_routes_and_contributions(
            &app,
            std::collections::HashMap::from([(pane, contribution)]),
        );
        assert!(
            tree.nodes.iter().any(|(id, _)| *id == contributed_child),
            "the subtree child remains in the composed update"
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.children().contains(&contributed_root)),
            "the registered pane leaf points at the contributed root"
        );
    }

    #[test]
    fn diagnostic_gloss_rows_attach_without_routes_and_retire_after_removal() {
        let mut app = App::test_stub();
        app.update(crate::action::Action::SummonPane(
            crate::panes::PaneKindId::new(crate::panes::kind::GLOSS),
        ));
        let pane = app
            .frisket
            .iter_leaves()
            .find_map(|(pane, content, _)| matches!(content, PaneContent::Gloss(_)).then_some(pane))
            .unwrap();
        app.diagnostic_inspection = Some(Ok(apparatus::Inspection {
            lines: vec![apparatus::InspectionLine {
                id: "apparatus/fixture/record/1".into(),
                text: "Loss evicted: 2".into(),
            }],
        }));
        let mut surface = crate::swatch_pane::SwatchPane::new(crate::swatch_pane::GLOSS_MINIMAP);
        surface.set_sections(vec![crate::sections::DIAGNOSTICS_SECTION]);
        surface.sync(&app, 480.0, 400.0);
        surface.scene(480, 400);
        let contribution = surface
            .diagnostic_tree(pane, crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0))
            .unwrap();
        let row_id = contribution
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Loss evicted: 2"))
            .unwrap()
            .0;
        let diagnostic_root = contribution.root;
        let (tree, routes) = project_app_with_routes_and_contributions(
            &app,
            std::collections::HashMap::from([(pane, contribution)]),
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(id, node)| *id == row_id && node.label() == Some("Loss evicted: 2")),
            "the platform tree must contain the actual diagnostic row"
        );
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.children().contains(&diagnostic_root)),
            "the Gloss leaf must attach its diagnostic subtree"
        );
        assert!(
            !routes.contains_key(&row_id),
            "read-only diagnostics create no activation route"
        );
        surface.set_sections(Vec::new());
        surface.sync(&app, 480.0, 400.0);
        surface.scene(480, 400);
        assert!(
            surface
                .diagnostic_tree(pane, crate::surface::Rect::new(100.0, 40.0, 480.0, 400.0))
                .is_none()
        );
        let (removed, routes) = project_app_with_routes(&app);
        assert!(!removed.nodes.iter().any(|(id, _)| *id == row_id));
        let before = app.graph_runtimes.graph().nodes().count();
        assert!(apply_route(&mut app, routes.get(&row_id), row_id).is_empty());
        assert_eq!(app.graph_runtimes.graph().nodes().count(), before);
        assert!(
            app.take_events().iter().any(|event| matches!(
                event,
                crate::observe::AppEvent::InteractionMissed {
                    what: "a11y-action",
                    ..
                }
            )),
            "a stale platform request stays an attributable miss"
        );
    }

    /// UIA stops traversal at a node whose children reference an id absent
    /// from the pushed tree, which reads to a person as "nothing after that
    /// when I hit down arrow". The in-process assert-a11y walk never caught
    /// this because it iterates the flat node list rather than following
    /// children.
    #[test]
    fn the_pushed_tree_has_no_dangling_children_and_no_duplicate_ids() {
        let app = App::test_stub();
        let tree = project_app(&app);
        let mut seen = HashSet::new();
        for (id, _) in &tree.nodes {
            assert!(seen.insert(*id), "duplicate node id {id:?} in one update");
        }
        for (id, node) in &tree.nodes {
            for child in node.children() {
                assert!(
                    seen.contains(child),
                    "node {id:?} references child {child:?} that is not in the update"
                );
            }
        }
        assert!(seen.contains(&tree.root), "the root itself is missing");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::action::{Action, Update};
    use crate::content::{ContentFacts, StructureFacts};

    fn app_with_live_doc() -> App {
        let mut app = App::test_stub();
        app.update(Action::OpenAddress("https://example.com/".to_string()));
        let node = app.graph_runtimes.focused_member().unwrap();
        app.update(Action::OpenInWorkbench);
        app.apply_update(Update::ContentSpawned {
            node,
            facts: Some(ContentFacts {
                engine: "genet.web".to_string(),
                lane: crate::content::ContentLane::Document,
                lineage: None,
                capabilities: Default::default(),
                structure: Some(StructureFacts {
                    title: Some("Example Domain".to_string()),
                    headings: 1,
                    links: 1,
                    outline: vec![
                        OutlineFact {
                            depth: 0,
                            role: "heading",
                            name: "Example Domain".to_string(),
                        },
                        OutlineFact {
                            depth: 1,
                            role: "link",
                            name: "More information...".to_string(),
                        },
                    ],
                }),
            }),
        });
        app
    }

    /// The stitched tree is coherent: one window root, every id unique
    /// (disjoint subtree ranges — the deletion-matrix bar), and every
    /// child id present in the node list.
    #[test]
    fn the_stitched_tree_is_coherent() {
        let app = app_with_live_doc();
        let tree = project_app(&app);
        let ids: Vec<_> = tree.nodes.iter().map(|(id, _)| *id).collect();
        let unique: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "no id collisions across subtrees");
        for (id, n) in &tree.nodes {
            for child in n.children() {
                assert!(
                    unique.contains(child),
                    "node {id:?} references a missing child {child:?}"
                );
            }
        }
        let (_, root) = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
        assert_eq!(root.role(), Role::Window);
        assert_eq!(root.label(), Some("Turnstone"));
    }

    /// The pane structure, the workbench tiling, and the live document's
    /// outline all arrive under the one root, and the document declares its
    /// partial capability instead of implying coverage.
    #[test]
    fn panes_workbench_and_documents_project_under_one_root() {
        let app = app_with_live_doc();
        let lines = a11y_lines(&app);
        let has = |s: &str| lines.iter().any(|l| l.contains(s));
        assert!(has("group: workbench"), "{lines:?}");
        assert!(has("tab: Tile"), "the tile's tab projects");
        assert!(has("document: Example Domain"), "{lines:?}");
        assert!(has("heading: Example Domain"));
        assert!(has("link: More information..."));
        assert!(!has("graph canvas, 1 nodes"), "inactive Orrery is not a visible canvas");
        let doc = project_app(&app)
            .nodes
            .into_iter()
            .map(|(_, n)| n)
            .find(|n| n.role() == Role::Document)
            .expect("the live document projects");
        assert_eq!(
            doc.description(),
            Some("structural outline only: no bounds, no per-element focus"),
            "the capability is declared, not implied"
        );
    }

    /// The omnibar joins the chrome subtree only while it is open, carrying
    /// its live text.
    #[test]
    fn the_omnibar_projects_while_open() {
        let mut app = App::test_stub();
        assert!(
            !a11y_lines(&app).iter().any(|l| l.starts_with("textinput")),
            "closed omnibar projects nothing"
        );
        app.update(Action::OmnibarOpen { command: false });
        app.update(Action::OmnibarChar('h'));
        app.update(Action::OmnibarChar('i'));
        let lines = a11y_lines(&app);
        assert!(
            lines.iter().any(|l| l == "textinput: omnibar = hi"),
            "{lines:?}"
        );
    }
}

#[cfg(test)]
mod reading_order_tests {
    use super::*;
    #[test]
    fn reading_order_follows_children_and_excludes_orphans() {
        let mut root = Node::new(Role::Document);
        root.set_label("page");
        root.set_children(vec![accesskit::NodeId(3), accesskit::NodeId(2)]);
        let named = |name: &str| {
            let mut n = Node::new(Role::Link);
            n.set_label(name);
            n
        };
        let tree = UxTree {
            root: accesskit::NodeId(1),
            nodes: vec![
                (accesskit::NodeId(2), named("second")),
                (accesskit::NodeId(4), named("orphan")),
                (accesskit::NodeId(1), root),
                (accesskit::NodeId(3), named("first")),
            ],
        };
        assert_eq!(
            tree_lines(&tree),
            ["document: page", "link: first", "link: second"]
        );
    }
}

#[cfg(test)]
mod micron_form_tests {
    use super::*;
    use crate::action::{Action, Effect};
    #[test]
    fn micron_host_editor_exposes_current_field_and_prompt_without_secrets() {
        let mut app = App::test_stub();
        let address = "7fc8950ec4e50695be76eddc8e539ee8:/page/index.mu";
        let source = "`[Submit`:/page/capture.mu`name|secret]\nName: `<name`seed>\nSecret: `<!|secret`mask-seed>\n";
        app.update(Action::OpenAddress(address.into()));
        let member = app.graph_runtimes.focused_member().unwrap();
        app.content.note_fetched(
            member,
            address.into(),
            crate::content::FetchedDocument {
                bytes: source.as_bytes().to_vec(),
                content_type: None,
                body: source.into(),
                effective_url: Some(address.into()),
                acquired_at_ms: 1,
            },
            source.len(),
        );
        app.update(Action::ComposeFocusedMicronForm);
        let focus = host_input_focus(&app).expect("the active editor owns host focus");
        let text = app.omnibar.text.clone();
        assert!(focus_host_input(&mut app, focus));
        assert_eq!(
            app.omnibar.text, text,
            "Focus does not reset or submit the form"
        );
        let host = project_app(&app).to_tree_update(host_input_focus(&app));
        let mut publication = crate::foreign_a11y::ForeignA11y::new();
        let safe = publication.initial_host(host.clone(), &[]).unwrap();
        let consumer = accesskit_consumer::Tree::new(safe, true);
        assert_eq!(
            consumer.state().focus().unwrap().label().as_deref(),
            Some("name")
        );
        assert!(
            host.nodes
                .iter()
                .find(|(id, _)| *id == focus)
                .unwrap()
                .1
                .supports_action(accesskit::Action::Focus)
        );
        publication.frame(host, &[]).unwrap();
        assert!(
            a11y_lines(&app)
                .iter()
                .any(|line| line == "textinput: name = seed")
        );
        assert!(
            a11y_lines(&app)
                .iter()
                .any(|line| line.contains("Enter to continue"))
        );
        app.update(Action::OmnibarCommit);
        let tree = project_app(&app);
        let host = tree.to_tree_update(host_input_focus(&app));
        let safe = publication.initial_host(host.clone(), &[]).unwrap();
        let consumer = accesskit_consumer::Tree::new(safe, true);
        let focused = consumer.state().focus().unwrap();
        assert_eq!(focused.label().as_deref(), Some("secret"));
        assert_eq!(focused.role(), Role::PasswordInput);
        assert_ne!(focused.value().as_deref(), Some("mask-seed"));
        publication.frame(host, &[]).unwrap();
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.role() == Role::PasswordInput && n.label() == Some("secret"))
        );
        assert!(!a11y_lines(&app).join("\n").contains("mask-seed"));
        app.update(Action::OmnibarCommit);
        assert!(
            a11y_lines(&app)
                .iter()
                .any(|line| line.contains("type send and press Enter"))
        );
        app.update(Action::OmnibarInsert("send".into()));
        let effects = app.update(Action::OmnibarCommit);
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, Effect::SubmitMicron { .. }))
                .count(),
            1
        );
    }
}

#[cfg(test)]
mod lens_window_tests {
    use super::*;
    use crate::panes::{FrisketId, FrisketLayout, GraphId, PaneId, PaneKindId, PaneNode};

    #[test]
    fn lens_projection_identity_keeps_retained_links_per_window_and_refuses_missing_lenses() {
        let mut app = App::test_stub();
        app.lenses.clear();
        for ordinal in 0..2 {
            let pane = PaneId(40 + ordinal as u64);
            app.lenses.push(Some(FrisketLayout {
                id: FrisketId::new(format!("lens-test-{ordinal}")),
                label: format!("Document {ordinal}"),
                root: PaneNode::Workspace(Box::new(
                    crate::panes::workspace_bridge::OuterWorkspace::single(
                        pane.0,
                        format!("Document {ordinal}"),
                        (PaneContent::Registered(PaneKindId::new("retained-test")), GraphId::nil()),
                    ),
                )),
            }));
        }
        let mut roots = Vec::new();
        for ordinal in 0..2 {
            let pane = PaneId(40 + ordinal as u64);
            let mut document = Node::new(Role::Group);
            document.set_label("retained document");
            let links = ["First Gemini", "Second Gemini"].into_iter().enumerate().map(|(index, name)| {
                let mut link = Node::new(Role::Link);
                link.set_label(name);
                stitch(&format!("test/lens/{ordinal}/link/{index}"), link, Vec::new())
            }).collect();
            let contribution = stitch(&format!("test/lens/{ordinal}/document"), document, links);
            let retained_ids: Vec<_> = contribution.nodes.iter().map(|(id, _)| *id).collect();
            let (tree, _) = project_lens_with_contributions(
                &app, ordinal, std::collections::HashMap::from([(pane, contribution)]),
            ).expect("a live shared Workspace lens projects its retained subtree");
            let window = tree.nodes.iter().find(|(id, _)| *id == tree.root).unwrap();
            assert_eq!(window.1.role(), Role::Window);
            let expected = format!("Turnstone lens {ordinal}");
            assert_eq!(window.1.label(), Some(expected.as_str()));
            assert!(retained_ids.iter().all(|id| tree.nodes.iter().any(|(node, _)| node == id)));
            let mut forest = crate::foreign_a11y::ForeignA11y::new();
            forest.frame(tree.to_tree_update(None), &[]).expect("actual composed host publication");
            assert!(matches_lines(&forest.observed_lines(), &format!(
                "window: {expected} >> link: First Gemini >> link: Second Gemini",
            )), "window identity and retained link order remain in the published forest");
            roots.push(tree.root);
        }
        assert_ne!(roots[0], roots[1], "distinct native lenses retain distinct roots");
        assert!(project_lens_with_contributions(&app, 2, Default::default()).is_none());
        app.lenses[1] = None;
        assert!(project_lens_with_contributions(&app, 1, Default::default()).is_none());
    }
}
