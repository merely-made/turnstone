//! The accessibility projection (rung 5 slice F): one stitched application
//! tree over app truth — chrome, the frisket pane structure, the workbench
//! tiling, and each live document's structural outline — with stable,
//! path-derived AccessKit ids (uxtree's scheme, so subtree ids stay disjoint
//! by construction).
//!
//! Supply, all pre-existing and previously unused: `crate::panes::project_frisket_with`
//! (the pane tree, with a per-leaf content hook), `mere::workbench::project_workbench`
//! (the tiling), and `uxtree::stitch`. The document subtree is built here from
//! the [`StructureFacts`] outline the content port mirrors at spawn (the
//! `DocumentSession::inspect` accessor landed with the Inspector slice) — so
//! the whole projection is PURE over `App`, testable headless, and the same
//! read the observation snapshot serves.
//!
//! Honesty (the no-placebo rule): this is `A11yCapability::Partial` by
//! declaration. The projection is structural — roles, names, and levels, no
//! bounds and no per-element focus — and each document root SAYS so in its
//! description rather than implying coverage it does not have. Pushing the
//! `TreeUpdate` to an OS adapter is a separate, later piece of work (the
//! donor never landed it either); producing the coherent tree is this rung's
//! deletion-matrix bar.

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
    project_app_capturing(app, &mut None)
}

fn project_app_capturing(app: &App, orrery_pane: &mut Option<crate::panes::PaneId>) -> UxTree {
    let chrome = project_chrome(app);
    // Live documents, in graph order (deterministic), each from its mirrored
    // structural outline. They stitch under the CANVAS leaf: content insets
    // and workbench tiles alike render over graph truth, and a document
    // without a structural read is announced without children (honest).
    let docs: Vec<UxTree> = app
        .graph_runtimes
        .graph()
        .nodes()
        .filter(|(_, n)| matches!(app.content.get(n.id), Some(NodeContent::Live)))
        .map(|(_, n)| project_live_document(app, n.id, n.url()))
        .collect();
    let mut docs = Some(docs);
    let panes = crate::panes::project_frisket_with(&app.frisket, |content, id| match content {
        PaneContent::Workbench => app
            .workbench_for_pane(id)
            .map(mere::workbench::project_workbench),
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
        }
        PaneContent::Registered(kind) if kind.as_str() == crate::panes::kind::FROZEN_PROJECTION => {
            Some(project_frozen_projection(app))
        }
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
    project_app(app)
        .nodes
        .iter()
        .map(|(_, n)| {
            let role = format!("{:?}", n.role()).to_lowercase();
            match (n.label(), n.value()) {
                (Some(label), Some(value)) => format!("{role}: {label} = {value}"),
                (Some(label), None) => format!("{role}: {label}"),
                (None, Some(value)) => format!("{role} = {value}"),
                (None, None) => role,
            }
        })
        .collect()
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

/// The chrome subtree: the omnibar (a text input when open, with its live
/// text and caret-free honesty) and the at-rest caption.
fn project_chrome(app: &App) -> UxTree {
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    if app.omnibar.open {
        let id = node_id_for_path("turnstone/chrome/omnibar");
        let mut n = Node::new(Role::TextInput);
        n.set_label("omnibar");
        n.set_value(app.omnibar.presented_text());
        nodes.push((id, n));
        children.push(id);
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
        input.set_value(app.document_find.query.clone());
        nodes.push((input_id, input));
        children.push(input_id);

        let status_id = node_id_for_path("turnstone/chrome/document-find/status");
        let mut status = Node::new(Role::Status);
        status.set_label(app.document_find.status());
        nodes.push((status_id, status));
        children.push(status_id);
    }
    if app.shell_chrome_config().projects_shellbar()
        && let Some(caption) = crate::app::focused_caption(&app.graph_runtimes)
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
    /// The chrome: open the omnibar in its find lane.
    OpenOmnibar,
    /// A frozen-projection instance: select that member in the graph pane, so
    /// a reader who found a node by name can make it the app's selection.
    SelectMember {
        pane: crate::panes::PaneId,
        member: uuid::Uuid,
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
    let mut orrery_pane = None;
    let tree = project_app_capturing(app, &mut orrery_pane);

    let mut routes = A11yRoutes::new();
    routes.insert(
        node_id_for_path("turnstone/chrome/omnibar"),
        A11yRoute::OpenOmnibar,
    );
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
        }
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
        }
        None => {
            app.note(crate::observe::AppEvent::InteractionMissed {
                what: "a11y-action",
                target: format!("{target:?}"),
            });
            Vec::new()
        }
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
                }
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
                lineage: None,
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
        assert!(has("graph canvas, 1 nodes"));
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
