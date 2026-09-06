//! Stable identity and retirement of document appearances. Pure host planning.

use super::{Rect, SurfaceKind};

pub(crate) fn assign_content_appearance_ids(
    surfaces: &mut [crate::surface::Surface],
    host_slot: u32,
    roles: &[(uuid::Uuid, u64, Rect)],
) {
    let mut used_roles = std::collections::HashSet::new();
    for surface in surfaces {
        if let SurfaceKind::Content(node) = surface.kind {
            let role = roles
                .iter()
                .enumerate()
                .find(|(index, (candidate, _, rect))| {
                    *candidate == node && *rect == surface.rect && !used_roles.contains(index)
                })
                .map(|(index, (_, role, _))| {
                    used_roles.insert(index);
                    *role
                })
                .unwrap_or(0);
            surface.id = crate::surface::SurfaceId::content_appearance(
                node,
                role ^ (host_slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            );
        }
    }
}

pub(crate) fn workbench_appearance_role(pane: crate::panes::PaneId, node: uuid::Uuid) -> u64 {
    let (high, low) = node.as_u64_pair();
    0x2000_0000_0000_0000 | pane.0.rotate_left(17) ^ high ^ low
}

pub(crate) fn reader_appearance_belongs_to_pane(
    id: crate::surface::SurfaceId,
    node: uuid::Uuid,
    pane: crate::panes::PaneId,
    lens_slots: usize,
) -> bool {
    let roles = [
        0x1000_0000_0000_0000 | pane.0,
        workbench_appearance_role(pane, node),
        0x3000_0000_0000_0000 | pane.0,
    ];
    // Lens slots retain their ordinal when a window closes. Include the
    // primary host and every slot so closing a pane removes only its own
    // appearances, including a suspended member, without disturbing peers.
    (0..=lens_slots).any(|host| {
        roles.iter().any(|role| {
            crate::surface::SurfaceId::content_appearance(
                node,
                role ^ (host as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            ) == id
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_duplicate_placements_receive_distinct_appearance_routes() {
        let node = uuid::Uuid::from_u128(42);
        let mut surfaces = crate::surface::assemble(
            &[],
            &[(node, Rect::new(0.0, 0.0, 180.0, 90.0))],
            Some((node, Rect::new(200.0, 0.0, 420.0, 240.0))),
            None,
        );
        let tile = Rect::new(0.0, 0.0, 180.0, 90.0);
        let inset = Rect::new(200.0, 0.0, 420.0, 240.0);
        assign_content_appearance_ids(
            &mut surfaces,
            0,
            &[
                (node, 0x1000_0000_0000_0007, tile),
                (node, 0x3000_0000_0000_0003, inset),
            ],
        );
        assert_eq!(surfaces.len(), 2);
        assert_ne!(surfaces[0].id, surfaces[1].id);
        assert_eq!(
            crate::surface::focus_for_press(
                &surfaces,
                crate::surface::FocusTarget::Graph(crate::panes::PaneId(0)),
                210.0,
                10.0,
            ),
            crate::surface::FocusTarget::Content {
                node,
                appearance: surfaces[1].id,
            },
            "input keeps the focused appearance identity rather than collapsing to its document"
        );

        let inset_id = surfaces[1].id;
        let mut surviving = crate::surface::assemble(&[], &[], Some((node, inset)), None);
        assign_content_appearance_ids(
            &mut surviving,
            0,
            &[(node, 0x3000_0000_0000_0003, inset)],
        );
        assert_eq!(
            surviving[0].id, inset_id,
            "closing a sibling appearance leaves this inset's retained identity intact"
        );

        let overlap = Rect::new(20.0, 20.0, 180.0, 90.0);
        let mut overlapping = crate::surface::assemble(
            &[(crate::surface::SurfaceKind::Content(node), overlap)],
            &[(node, overlap)],
            None,
            None,
        );
        assign_content_appearance_ids(
            &mut overlapping,
            0,
            &[
                (node, 0x1000_0000_0000_0001, overlap),
                (node, 0x1000_0000_0000_0002, overlap),
            ],
        );
        assert_ne!(overlapping[0].id, overlapping[1].id);
    }

    #[test]
    fn reader_closed_pane_cleanup_preserves_other_appearances() {
        let node = uuid::Uuid::from_u128(42);
        let closed = crate::panes::PaneId(7);
        let surviving = crate::panes::PaneId(9);
        for role in [
            0x1000_0000_0000_0000 | closed.0,
            workbench_appearance_role(closed, node),
            0x3000_0000_0000_0000 | closed.0,
        ] {
            for host in 0_u64..=2 {
                let id = crate::surface::SurfaceId::content_appearance(
                    node,
                    role ^ host.wrapping_mul(0x9e37_79b9_7f4a_7c15),
                );
                assert!(reader_appearance_belongs_to_pane(
                    id, node, closed, 2
                ));
                assert!(!reader_appearance_belongs_to_pane(
                    id, node, surviving, 2
                ));
            }
        }
    }

}
