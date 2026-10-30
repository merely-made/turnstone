// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use std::cell::Cell;

thread_local! {
    static GEOMETRY_VISITS: Cell<usize> = const { Cell::new(0) };
    static SEMANTIC_PROBES: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn geometry_visit() {
    GEOMETRY_VISITS.with(|count| count.set(count.get() + 1));
}

pub(super) fn semantic_probe() {
    SEMANTIC_PROBES.with(|count| count.set(count.get() + 1));
}

#[test]
fn accessibility_projection_host_work_is_linear() {
    for controls in [32, 64] {
        let mut dom = ScriptedDom::new();
        let html = (0..controls)
            .map(|index| format!("<button>Control {index}</button>"))
            .collect::<String>();
        let document = dom.document();
        dom.set_inner_html(document, &html);
        let mut layout = RetainedLayout::new();
        let _ = layout.scene(&mut dom, "button { height:20px; }", 400, 300);
        let mut dom_nodes = 0;
        walk_dom(&dom, document, &mut |_| dom_nodes += 1);
        GEOMETRY_VISITS.with(|count| count.set(0));
        SEMANTIC_PROBES.with(|count| count.set(0));
        let (tree, actions) = layout.accessibility_tree(&dom, None).unwrap();
        assert!(tree.nodes.len() >= controls);
        assert!(actions.len() >= controls);
        let geometry = GEOMETRY_VISITS.with(Cell::get);
        let semantics = SEMANTIC_PROBES.with(Cell::get);
        assert!(
            geometry > 0 && geometry <= dom_nodes * 2,
            "{controls} controls / {dom_nodes} DOM nodes made {geometry} geometry visits"
        );
        assert!(
            semantics > 0 && semantics <= dom_nodes * 2,
            "{controls} controls / {dom_nodes} DOM nodes made {semantics} semantic probes"
        );
    }
}

#[test]
fn bulk_projection_matches_point_geometry_after_scoped_retained_changes() {
    let mut dom = ScriptedDom::new();
    let document = dom.document();
    dom.set_inner_html(document, r#"
        <div id="outside"><div id="owner">
          <div id="scroll"><button>Move</button>
            <div role="textbox" data-cambium-text-value="Committed" aria-multiline="true">Draft</div>
            <button id="hidden">Hidden</button>
          </div>
        </div></div>"#);
    let by_id = |id| {
        let mut found = None;
        walk_dom(&dom, document, &mut |node| {
            if dom.attribute(node, &Namespace::default(), &LocalName::from("id")) == Some(id) {
                found = Some(node);
            }
        });
        found.unwrap()
    };
    let owner = by_id("owner");
    let scroll = by_id("scroll");
    let button = taproot::matching_estimated_fixture(&dom, &taproot::Selector::role("button"))[0];
    let sheet = "#outside { transform:translate(13px,7px); } #owner { width:160px; transform:translate(10%,9px); } #scroll { transform:translate(3px,4px); } button { width:80px;height:25px; } #hidden { visibility:hidden; } #scroll.moved { transform:translate(21px,17px); }";
    let mut retained = RetainedLayout::new();
    retained.set_scale(2.0);
    for restyle in [false, true] {
        if restyle {
            dom.set_attribute(scroll, qual("class"), "moved");
        }
        let _ = retained.scene(&mut dom, sheet, 800, 600);
        retained.viewport_scroll = (5.0, 6.0);
        retained.element_scroll.insert(owner, (2.0, 3.0));
        retained.element_scroll.insert(scroll, (7.0, 8.0));
        let projection = retained
            .document_projection_at(&dom, owner, Some(button))
            .unwrap();
        let mut point_rects = HashMap::new();
        walk_dom(&dom, owner, &mut |node| {
            point_rects.insert(dom.opaque_id(node), retained.painted_rect(&dom, node));
        });
        for semantic in projection.nodes() {
            let expected = point_rects[&semantic.id.get()];
            assert_eq!(
                semantic
                    .bounds
                    .map(|rect| (rect.x, rect.y, rect.width, rect.height)),
                expected
            );
        }
        assert!(projection.nodes().iter().any(|node| node.state.focused));
        let field = projection
            .nodes()
            .iter()
            .find(|node| node.role == document_session_api::DocumentA11yRole::TextField)
            .unwrap();
        assert_eq!(field.value.as_deref(), Some("Committed"));
        assert!(field.state.editable && field.state.multiline);
        assert!(projection.nodes().iter().any(|node| node.state.hidden));
        let mut fresh = RetainedLayout::new();
        fresh.set_scale(2.0);
        let _ = fresh.scene(&mut dom, sheet, 800, 600);
        fresh.viewport_scroll = retained.viewport_scroll;
        fresh.element_scroll = retained.element_scroll.clone();
        assert_eq!(
            projection.nodes(),
            fresh
                .document_projection_at(&dom, owner, Some(button))
                .unwrap()
                .nodes()
        );
    }
}
