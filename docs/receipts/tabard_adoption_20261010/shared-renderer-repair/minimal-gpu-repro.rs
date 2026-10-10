use genet_render_host::{PendingRgbaReadback, RenderCore, RgbaFrame};
use netrender::{ColorLoad, NetrenderOptions, Scene};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
const N: u32 = 256;
const IMAGE: u64 = 0xc000000074617011;
const RASTER: u64 = 0x7461620070610001;
fn raster(core: &RenderCore, scene: &Scene, key: u64) -> wgpu::Texture {
    core.rasterize_scaled_for(
        key,
        scene,
        N,
        N,
        ColorLoad::Clear(wgpu::Color::TRANSPARENT),
        1.0,
    )
    .0
}
fn collect(mut p: PendingRgbaReadback, armed: Instant) -> RgbaFrame {
    loop {
        assert!(
            armed.elapsed() < Duration::from_secs(5),
            "copy exceeded 5 seconds"
        );
        let t = Instant::now();
        let f = p.poll();
        assert!(t.elapsed() < Duration::from_millis(250), "blocking poll");
        assert!(armed.elapsed() < Duration::from_secs(5));
        if let Some(f) = f {
            return f.unwrap();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn png(path: &Path, f: &RgbaFrame) {
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    let mut e = png::Encoder::new(file, f.width, f.height);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&f.rgba).unwrap();
}
fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir(&out).unwrap();
    let core = RenderCore::boot(NetrenderOptions {
        enable_vello: true,
        tile_cache_size: Some(64),
        ..Default::default()
    })
    .unwrap();
    let adapter = core.renderer().wgpu_device.core.adapter.get_info();
    eprintln!("ADAPTER {adapter:?}");
    let mut source = Scene::new(N, N);
    for (x, y, c) in [
        (0., 0., [1., 0., 0., 1.]),
        (128., 0., [0., 1., 0., 1.]),
        (0., 128., [0., 0., 1., 1.]),
        (128., 128., [1., 1., 0., 1.]),
    ] {
        source.push_rect(x, y, x + 128., y + 128., c);
    }
    let source_texture = raster(&core, &source, RASTER);
    core.renderer()
        .insert_image_vello(IMAGE, Arc::new(source_texture.clone()));
    let mut image = Scene::new(N, N);
    image.push_image_full(
        0.,
        0.,
        256.,
        256.,
        [0., 0., 1., 1.],
        [1.; 4],
        IMAGE,
        0,
        netrender::NO_CLIP,
    );
    eprintln!("STEP image before patchless");
    let before = raster(&core, &image, RASTER + 1);
    let mut solid = Scene::new(N, N);
    solid.push_rect(0., 0., 256., 256., [0.25, 0.5, 0.75, 1.]);
    eprintln!("STEP patchless solid interleave");
    let solid_texture = raster(&core, &solid, RASTER + 2);
    eprintln!("STEP same image after patchless fresh raster key");
    let after = raster(&core, &image, RASTER + 3);
    eprintln!("STEP same image repeat fresh raster key");
    let repeat = raster(&core, &image, RASTER + 4);
    let mut pending = Vec::new();
    for (name, texture) in [
        ("source", source_texture),
        ("image-before", before),
        ("patchless-solid", solid_texture),
        ("image-after", after),
        ("image-repeat", repeat),
    ] {
        let armed = Instant::now();
        pending.push((
            name,
            core.start_rgba8_readback(&texture, N, N).unwrap(),
            armed,
        ));
    }
    let mut frames = Vec::new();
    for (name, p, armed) in pending.into_iter().rev() {
        let frame = collect(p, armed);
        let ms = armed.elapsed().as_millis();
        eprintln!("MAP {name} {ms}ms");
        frames.push((name, frame, ms));
    }
    let baseline = &frames
        .iter()
        .find(|(name, _, _)| *name == "image-before")
        .unwrap()
        .1
        .rgba;
    let mut receipts = Vec::new();
    for (name, f, ms) in &frames {
        png(&out.join(format!("{name}.png")), f);
        std::fs::write(out.join(format!("{name}.rgba")), &f.rgba).unwrap();
        let changed = f
            .rgba
            .chunks_exact(4)
            .zip(baseline.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        let colors: std::collections::BTreeSet<_> = f
            .rgba
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2], p[3]])
            .collect();
        receipts.push(serde_json::json!({"name":name,"map_ms":ms,"changed_pixels_from_image_before":changed,"colors":colors,"width":f.width,"height":f.height,"corners":[&f.rgba[0..4],&f.rgba[1020..1024],&f.rgba[261120..261124],&f.rgba[262140..262144]]}));
    }
    let json = serde_json::json!({"adapter":format!("{adapter:?}"),"renderer":"Vello491 warm immutable rlibs","tile_size":64,"image_key":format!("{IMAGE:#x}"),"sequence":"solid quadrant source -> register -> image -> patchless solid -> same image fresh key -> same image another fresh key -> arm all copies -> collect all -> write","copies":receipts});
    std::fs::write(
        out.join("receipt.json"),
        serde_json::to_vec_pretty(&json).unwrap(),
    )
    .unwrap();
    eprintln!("RESULT {json}");
}
