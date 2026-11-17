use ao_formats::csm::AoMap;
use ao_snap::{assert_similar, compare, find_recursos, SnapEntity, Snapshot};
use image::{Rgba, RgbaImage};
use std::time::Instant;

fn hero() -> Vec<SnapEntity> {
    vec![SnapEntity { x: 50, y: 50, body: 1, head: 1, heading: 3, name: Some("Heroe".into()) }]
}

#[test]
fn compare_synthetic_images() {
    let a = RgbaImage::from_pixel(10, 10, Rgba([10, 20, 30, 255]));
    assert_eq!(compare(&a, &a.clone()).max_delta, 0);
    assert_eq!(compare(&a, &a.clone()).differing_ratio, 0.0);

    let mut b = a.clone();
    b.put_pixel(0, 0, Rgba([10, 20, 130, 255]));
    b.put_pixel(1, 0, Rgba([11, 20, 30, 255]));
    let d = compare(&a, &b);
    assert_eq!(d.max_delta, 100);
    assert!((d.differing_ratio - 0.02).abs() < 1e-9);
    // With tolerance 5 only the big change counts.
    let d = ao_snap::compare_with(&a, &b, 5);
    assert!((d.differing_ratio - 0.01).abs() < 1e-9);

    let c = RgbaImage::new(5, 5);
    assert_eq!(compare(&a, &c).differing_ratio, 1.0);
}

#[test]
fn golden_roundtrip_and_diff_file() {
    let dir = std::env::temp_dir().join(format!("ao_snap_test_{}", std::process::id()));
    let golden = dir.join("g.png");
    let img = RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255]));
    assert_similar(&img, &golden, 0.0); // creates
    assert!(golden.exists());
    assert_similar(&img, &golden, 0.0); // matches
    let mut other = img.clone();
    other.put_pixel(3, 3, Rgba([200, 2, 3, 255]));
    assert_similar(&other, &golden, 0.05); // 1/64 differing is allowed
    let r = std::panic::catch_unwind(|| assert_similar(&other, &golden, 0.0));
    assert!(r.is_err());
    assert!(dir.join("g.diff.png").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ullathorpe_matches_golden() {
    let Some(root) = find_recursos() else {
        eprintln!("AO resources not found, skipping");
        return;
    };
    let map = AoMap::load(&root, 1).expect("map 1");
    let mut snap = Snapshot::new(root);
    let img = snap.render(&map, (50, 50), (23, 18), &hero(), 0.0);
    assert_eq!(img.dimensions(), (23 * 32, 18 * 32));
    let golden = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/ullathorpe_50_50.png");
    assert_similar(&img, golden, 0.001);
}

#[test]
fn capture_is_fast_once_textures_are_loaded() {
    let Some(root) = find_recursos() else {
        eprintln!("AO resources not found, skipping");
        return;
    };
    let map = AoMap::load(&root, 1).expect("map 1");
    let mut snap = Snapshot::new(root);
    snap.render(&map, (50, 50), (23, 18), &hero(), 0.0); // warms the texture cache
    let t = Instant::now();
    let img = snap.render(&map, (50, 50), (23, 18), &hero(), 0.0);
    let us = t.elapsed().as_micros();
    let ms = us / 1000;
    eprintln!("warm capture: {us} us");
    assert_eq!(img.width(), 23 * 32);
    if !cfg!(debug_assertions) {
        assert!(ms < 300, "capture took {ms} ms");
    }
}

#[test]
fn scale_doubles_the_image() {
    let Some(root) = find_recursos() else { return };
    let map = AoMap::load(&root, 1).expect("map 1");
    let mut snap = Snapshot::new(root).with_scale(2);
    let img = snap.render(&map, (50, 50), (5, 5), &[], 0.0);
    assert_eq!(img.dimensions(), (5 * 64, 5 * 64));
}
