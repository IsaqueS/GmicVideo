//! `fx_vignette strength,min_radius%,max_radius%,r,g,b[,a]`
//!
//! G'MIC: a `d x d` distance-to-centre map (d = max(w,h)) is resized to `w x h` with a
//! moving average, cut to [min%, max%] of its range, normalised to [0, strength] and
//! subtracted from an opaque alpha; the image is then blended over the vignette colour.
//! (G'MIC also forces pixel (0,0) to alpha 0 — reproduced for identical output.)

use super::{arg, GpuEffect};
use crate::gpu::PassChain;

pub fn supports(_args: &[f64]) -> bool {
    true
}

pub struct Vignette;

const BODY: &str = r#"
// arg: 0 strength, 1 lo, 2 hi, 3..5 colour rgb, 6 colour alpha (0..255), 7 d, 8 centre
fn dist_at(i: f32, j: f32) -> f32 {
    let c = arg(8u);
    return sqrt((i - c) * (i - c) + (j - c) * (j - c));
}

// Moving-average resize: average of the d x d map over the source block of output pixel p.
fn block_avg(p: vec2<i32>) -> f32 {
    let d = arg(7u);
    let r = vec2<f32>(d, d) / size_f();
    let a = vec2<f32>(p) * r;
    let b = a + r;
    var acc = 0.0;
    var wsum = 0.0;
    for (var j = i32(floor(a.y)); f32(j) < b.y; j = j + 1) {
        let wy = min(b.y, f32(j + 1)) - max(a.y, f32(j));
        for (var i = i32(floor(a.x)); f32(i) < b.x; i = i + 1) {
            let wx = min(b.x, f32(i + 1)) - max(a.x, f32(i));
            let w = wx * wy;
            acc = acc + w * dist_at(f32(i), f32(j));
            wsum = wsum + w;
        }
    }
    return acc / max(wsum, 1e-6);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let img = textureLoad(src, p, 0);
    let lo = arg(1u);
    let hi = arg(2u);
    let dc = clamp(block_avg(p), lo, hi);
    let n = select(0.0, (dc - lo) / (hi - lo) * arg(0u), hi > lo);
    var alpha = clamp(255.0 - n, 0.0, 255.0);
    if (p.x == 0 && p.y == 0) { alpha = 0.0; }
    let a = alpha / 255.0;
    let ca = arg(6u) / 255.0;
    let col = vec3<f32>(arg(3u), arg(4u), arg(5u));
    // Image (alpha a) over the colour layer (alpha ca).
    let out_a = a + ca * (1.0 - a);
    let rgb = select(vec3<f32>(0.0), (img.rgb * a + col * ca * (1.0 - a)) / out_a, out_a > 0.0);
    textureStore(dst, p, vec4<f32>(rgb, 255.0));
}
"#;

/// Exact moving-average value of the resized distance map at output pixel (x, y) (CPU side).
fn block_avg(x: u32, y: u32, w: u32, h: u32, d: f64, c: f64) -> f64 {
    let (rx, ry) = (d / w as f64, d / h as f64);
    let (ax, ay) = (x as f64 * rx, y as f64 * ry);
    let (bx, by) = (ax + rx, ay + ry);
    let (mut acc, mut wsum) = (0.0, 0.0);
    let mut j = ay.floor();
    while j < by {
        let wy = by.min(j + 1.0) - ay.max(j);
        let mut i = ax.floor();
        while i < bx {
            let wx = bx.min(i + 1.0) - ax.max(i);
            let wgt = wx * wy;
            acc += wgt * ((i - c).powi(2) + (j - c).powi(2)).sqrt();
            wsum += wgt;
            i += 1.0;
        }
        j += 1.0;
    }
    acc / wsum.max(1e-12)
}

impl GpuEffect for Vignette {
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        let (w, h) = (chain.width(), chain.height());
        let d = w.max(h) as f64;
        let c = ((d - 1.0) / 2.0).round();

        // Value range of the resized map: max at a corner, min next to the centre.
        let corners = [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
        let max_d = corners.iter().map(|&(x, y)| block_avg(x, y, w, h, d, c)).fold(f64::MIN, f64::max);
        let (cx, cy) = ((c + 0.5) * w as f64 / d, (c + 0.5) * h as f64 / d);
        let mut min_d = f64::MAX;
        for y in (cy as i64 - 2).max(0)..=(cy as i64 + 2).min(h as i64 - 1) {
            for x in (cx as i64 - 2).max(0)..=(cx as i64 + 2).min(w as i64 - 1) {
                min_d = min_d.min(block_avg(x as u32, y as u32, w, h, d, c));
            }
        }
        let (p_lo, p_hi) = (arg(args, 1, 70.0) / 100.0, arg(args, 2, 95.0) / 100.0);
        let lo = min_d + p_lo * (max_d - min_d);
        let hi = min_d + p_hi * (max_d - min_d);

        let mut params = chain.params();
        params
            .set(0, arg(args, 0, 70.0))
            .set(1, lo)
            .set(2, hi)
            .set(3, arg(args, 3, 0.0))
            .set(4, arg(args, 4, 0.0))
            .set(5, arg(args, 5, 0.0))
            .set(6, arg(args, 6, 255.0))
            .set(7, d)
            .set(8, c);
        chain.pass("vignette", BODY, &params);
        Ok(())
    }
}
