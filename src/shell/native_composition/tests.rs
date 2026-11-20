// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;
fn mask_scene(blur: f32) -> paint_list_render::TranslatedDisplayList {
    let logical = 0xFFFF_0000_0000_0000;
    let mut scene = netrender::Scene::default();
    scene
        .ops
        .push(netrender::SceneOp::Image(netrender::SceneImage {
            x0: 0.0,
            y0: 0.0,
            x1: 30.0,
            y1: 40.0,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            key: logical,
            transform_id: 0,
            clip_rect: netrender::NO_CLIP,
            clip_corner_radii: [0.0; 4],
            clamp_to_uv: false,
            nearest: false,
        }));
    paint_list_render::TranslatedDisplayList {
        scene,
        external_textures: Vec::new(),
        box_shadow_masks: vec![paint_list_render::BoxShadowMaskRequest {
            key: logical,
            dim: 64,
            bounds: [3.0, 4.0, 28.0, 35.0],
            corner_radius: 2.0,
            blur_radius_px: blur,
            invert: false,
        }],
    }
}
#[test]
fn mask_cache_reuses_exact_requests_and_refreshes_changed_blur_or_device() {
    let mut masks = Masks::new();
    let mut builds = Vec::new();
    let mut retired = Vec::new();
    let mut first = mask_scene(4.0);
    stage_masks(
        &mut masks,
        0,
        71,
        1,
        &mut first,
        |mask| builds.push(mask.key),
        |key| retired.push(key),
    )
    .unwrap_or_else(|_| panic!("first mask"));
    let key = first.box_shadow_masks[0].key;
    assert!(matches!(&first.scene.ops[0], netrender::SceneOp::Image(image) if image.key==key));
    for (blur, generation, expected_builds) in [(4.0, 1, 1), (9.0, 1, 2), (9.0, 2, 3)] {
        let mut scene = mask_scene(blur);
        stage_masks(
            &mut masks,
            0,
            71,
            generation,
            &mut scene,
            |mask| builds.push(mask.key),
            |key| retired.push(key),
        )
        .unwrap_or_else(|_| panic!("cached mask"));
        assert_eq!(scene.box_shadow_masks[0].key, key);
        assert_eq!(builds.len(), expected_builds);
    }
    assert!(retired.is_empty());
    let mut absent = mask_scene(9.0);
    absent.box_shadow_masks.clear();
    absent.scene.ops.clear();
    stage_masks(
        &mut masks,
        0,
        71,
        2,
        &mut absent,
        |mask| builds.push(mask.key),
        |key| retired.push(key),
    )
    .unwrap_or_else(|_| panic!("mask withdrawal"));
    assert_eq!(retired, [key]);
    assert!(masks[&0].is_empty());
}
#[test]
fn equal_mask_ordinals_keep_distinct_window_and_session_custody() {
    let mut masks = Masks::new();
    let mut builds = Vec::new();
    let mut retired = Vec::new();
    let mut keys = Vec::new();
    for (window, session) in [(0, 71), (1, 71), (0, 72)] {
        let mut scene = mask_scene(4.0);
        stage_masks(
            &mut masks,
            window,
            session,
            1,
            &mut scene,
            |mask| builds.push(mask.key),
            |key| retired.push(key),
        )
        .unwrap_or_else(|_| panic!("independent mask"));
        keys.push(scene.box_shadow_masks[0].key);
    }
    assert_ne!(keys[0], keys[1]);
    assert_ne!(keys[0], keys[2]);
    assert_ne!(keys[1], keys[2]);
    assert_eq!(retired, [keys[0]], "the replaced session alone retires");
    assert_eq!(masks[&1][&(71, 0xFFFF_0000_0000_0000)].key, keys[1]);
    assert!(
        keys.iter()
            .all(|key| key & 0xFFFF_FFFF_0000_0000 == MASK_BASE)
    );
}
#[test]
fn allocated_producers_preserve_custody_and_rootstock_range() {
    let mut images = Images::new();
    let first = allocate(&images, 11).unwrap_or_else(|_| panic!("first allocation"));
    images.entry(0).or_default().insert(
        (7, 11),
        ProducerImage {
            key: first,
            epoch: 1,
            size: [20, 30],
            direct: false,
        },
    );
    let second = allocate(&images, 12).unwrap_or_else(|_| panic!("second allocation"));
    assert_ne!(first, second);
    assert!(first > 0 && first < 1 << 61);
    assert!(second > 0 && second < 1 << 61);
    assert_eq!(images[&0][&(7, 11)].key, first);
    // Exercise the actual renderer key conversion's domain assertion.
    let _ = external_image_key(first);
    let _ = external_image_key(second);
}
