//! Image comparison against golden references.
use image::RgbaImage;
use std::path::{Path, PathBuf};

/// Result of comparing two images.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diff {
    /// Fraction of pixels (0..=1) whose largest channel difference exceeds the channel tolerance.
    /// Images of different size count as fully different (1.0).
    pub differing_ratio: f64,
    /// Largest per-channel difference found anywhere (0..=255).
    pub max_delta: u8,
}

/// Exact comparison (channel tolerance 0).
pub fn compare(a: &RgbaImage, b: &RgbaImage) -> Diff {
    compare_with(a, b, 0)
}

/// A pixel counts as different when any channel differs by more than `channel_tolerance`.
pub fn compare_with(a: &RgbaImage, b: &RgbaImage, channel_tolerance: u8) -> Diff {
    if a.dimensions() != b.dimensions() {
        return Diff { differing_ratio: 1.0, max_delta: 255 };
    }
    let (mut bad, mut max) = (0usize, 0u8);
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        let d = pa.0.iter().zip(pb.0.iter()).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        max = max.max(d);
        if d > channel_tolerance {
            bad += 1;
        }
    }
    let total = (a.width() as usize * a.height() as usize).max(1);
    Diff { differing_ratio: bad as f64 / total as f64, max_delta: max }
}

/// Per-channel tolerance used by [`assert_similar`] (absorbs PNG/rounding noise only).
pub const GOLDEN_CHANNEL_TOLERANCE: u8 = 2;

fn sibling(golden: &Path, suffix: &str) -> PathBuf {
    golden.with_extension(suffix)
}

/// Compares `img` with the golden PNG at `golden`.
///
/// - Missing reference (or `AO_SNAP_UPDATE=1`): writes it and returns (a note goes to stderr).
/// - Otherwise panics if more than `max_ratio` of the pixels differ, after saving
///   `<name>.diff.png` (red = changed pixels over a dimmed copy) and `<name>.actual.png` next to it.
pub fn assert_similar(img: &RgbaImage, golden: impl AsRef<Path>, max_ratio: f64) {
    let golden = golden.as_ref();
    let update = std::env::var("AO_SNAP_UPDATE").is_ok_and(|v| v == "1");
    if update || !golden.exists() {
        if let Some(dir) = golden.parent() {
            std::fs::create_dir_all(dir).expect("create golden dir");
        }
        img.save(golden).expect("write golden image");
        eprintln!("ao-snap: reference {} {}", if update { "updated:" } else { "created:" }, golden.display());
        return;
    }
    let reference = image::open(golden).expect("read golden image").to_rgba8();
    let d = compare_with(img, &reference, GOLDEN_CHANNEL_TOLERANCE);
    if d.differing_ratio <= max_ratio {
        return;
    }
    let actual = sibling(golden, "actual.png");
    let diff_path = sibling(golden, "diff.png");
    img.save(&actual).ok();
    if img.dimensions() == reference.dimensions() {
        let mut out = img.clone();
        for (o, r) in out.pixels_mut().zip(reference.pixels()) {
            let delta = o.0.iter().zip(r.0.iter()).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            *o = if delta > GOLDEN_CHANNEL_TOLERANCE {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([o[0] / 3, o[1] / 3, o[2] / 3, 255])
            };
        }
        out.save(&diff_path).ok();
    }
    panic!(
        "image differs from {}: {:.4}% pixels differ (max delta {}, allowed {:.4}%); see {} and {}",
        golden.display(),
        d.differing_ratio * 100.0,
        d.max_delta,
        max_ratio * 100.0,
        diff_path.display(),
        actual.display()
    );
}
