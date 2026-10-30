// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// [`scene_from_dom`] plus custom-paint leaves (the Gloss minimap swatch): size
/// each `<custom-leaf>` from its laid-out box, re-paint the dirty ones through
/// the registry into `cache`, and splice their commands at their boxes. The
/// leaf-render pipeline the surfaces-in-cambium plan lists as what the host
/// still owed.
///
/// **One-shot.** This builds a layout, uses it once and drops it, which is
/// right for a test or a single measurement and wrong for anything that
/// paints repeatedly: a renderer calling this per frame re-cascades and
/// re-shapes its whole document to draw an unchanged screen. Panes go
/// through [`RetainedLayout`] instead.
pub fn scene_from_dom_with_leaves(
    dom: &ScriptedDom,
    sheet: &str,
    w: u32,
    h: u32,
    registry: &mut sprigging::LeafRegistry<u64>,
    cache: &mut sprigging::RenderedLeaves,
) -> netrender::Scene {
    let mut snapshot = build_livery_snapshot(dom, sheet, w, h, TextSystem::new());
    let mut sizes = HashMap::new();
    walk_dom(dom, dom.document(), &mut |node| {
        if let (Some(key), Some(fragment)) =
            (custom_leaf_key(dom, node), snapshot.fragments.get(node))
        {
            sizes.insert(key, (fragment.width, fragment.height));
        }
    });
    registry.render_into(
        |key| {
            sizes.get(&key).map(|&(w, h)| sprigging::Size {
                width: w,
                height: h,
            })
        },
        cache,
    );
    let viewport = DeviceIntSize::new(w as i32, h as i32);
    let mut plist = paint_snapshot(dom, &mut snapshot, w, h, &HashMap::new(), (0.0, 0.0));
    walk_dom(dom, dom.document(), &mut |node| {
        let Some(key) = custom_leaf_key(dom, node) else {
            return;
        };
        let Some((x, y, _, _)) =
            snapshot_node_rect(dom, &snapshot, node, &HashMap::new(), (0.0, 0.0))
        else {
            return;
        };
        if let Some(commands) = cache.get(key) {
            plist.push_host_commands_at(paint_list_api::LayoutPoint::new(x, y), commands);
        }
    });
    composite(viewport, &plist)
}
