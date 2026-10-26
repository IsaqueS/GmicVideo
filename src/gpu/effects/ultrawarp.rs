//! UltraWarp 2 (`fx_ultrawarptwo`, G'MIC `ultrawarptwo`) on the GPU.
//!
//! Each iteration: warp the image by its own intensities (`fx_warp_by_intensity`: every pixel
//! is fetched from an offset proportional to its value — per channel, or by the RGB norm),
//! optionally negate (`v = max - v`), then mix in HSV/HSI/HSL with random factors and shifts.
//!
//! The random choices (interpolation, boundary, factors, shifts, negation) are per-iteration
//! scalars. They are drawn on the CPU from a generator seeded with the frame index, so frames
//! are repeatable but use a different random sequence than G'MIC. With all "Range" values at
//! 0 and no "Random" choices the result matches G'MIC.
//!
//! Supported: "From RGB To" / "To RGB From" = RGB, warp channels None/All/RGB/R/G/B.
//! Other colour spaces fall back to G'MIC on the CPU.
//!
//! Arguments (`args[k]` = G'MIC `$k`, `args[0]` is the GUI's Recompute button):
//! 1 iterations · 2 from-RGB space · 3–10 x/y factor & offset (centre, range) · 11 correlated ·
//! 12 interpolation · 13 boundary · 14 warp channels · 15 alpha · 16 to-RGB space ·
//! 17 random negation · 18 HSX every iteration · 19 X channel (V/I/L) · 20–27 factors ·
//! 28–35 shifts (centre, range) for hue, saturation, X channel, alpha.

use super::{arg, GpuEffect};
use crate::gpu::PassChain;

pub fn supports(args: &[f64]) -> bool {
    let rgb_from = arg(args, 2, 1.0).round() as i64 == 1;
    let rgb_to = arg(args, 16, 1.0).round() as i64 == 1;
    let chann = arg(args, 14, 3.0).round() as i64;
    rgb_from && rgb_to && matches!(chann, 0 | 2 | 3 | 4 | 5 | 6)
}

#[derive(Default)]
pub struct UltraWarp2;

/// Small deterministic generator for the per-iteration random values.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        // SplitMix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
    /// G'MIC `u(a,b)`.
    fn u(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// arg: 0 xff, 1 yff, 2 xofff, 3 yofff, 4 norm mode (1) / per channel (0), 5 interpolation,
/// 6 boundary, 7 channel mask (bit 0 = R, 1 = G, 2 = B), 8 single-channel mode
const WARP: &str = r#"
fn uw_mod(a: f32, b: f32) -> f32 { return a - b * floor(a / b); }

// Lookups as G'MIC's math parser does them: nearest truncates the coordinate; periodic
// linear interpolation first wraps the coordinate into [0, size - 0.5).
fn sample(x: f32, y: f32, interp: i32, boundary: i32) -> vec4<f32> {
    let s = size_f();
    if (interp == 0) {
        return fetch(src, vec2<i32>(i32(trunc(x)), i32(trunc(y))), boundary);
    }
    if (boundary == 2) {
        return bilinear(src, uw_mod(x, s.x - 0.5), uw_mod(y, s.y - 0.5), 2);
    }
    return bilinear(src, x, y, boundary);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let xff = arg(0u);
    let yff = arg(1u);
    let xo = arg(2u);
    let yo = arg(3u);
    let norm_mode = arg(4u) > 0.5;
    let interp = i32(arg(5u));
    let boundary = i32(arg(6u));
    let mask = u32(arg(7u));
    let single = arg(8u) > 0.5;
    let c = textureLoad(src, p, 0).rgb;
    let fp = vec2<f32>(p);
    var out = c;
    if (norm_mode && !single) {
        let ni = length(c);
        out = sample(fp.x + (ni - xo) * xff, fp.y + (ni - yo) * yff, interp, boundary).rgb;
    } else {
        for (var ch = 0u; ch < 3u; ch = ch + 1u) {
            if (((mask >> ch) & 1u) == 0u) { continue; }
            // Single channel: G'MIC sees a 1-channel image, so norm(R,G,B) = |value|.
            let v = select(c[ch], abs(c[ch]), norm_mode);
            out[ch] = sample(fp.x + (v - xo) * xff, fp.y + (v - yo) * yff, interp, boundary)[ch];
        }
    }
    textureStore(dst, p, vec4<f32>(out, 255.0));
}
"#;

/// v = max - v (max over the whole image, reduction slot arg0).
const NEGATE: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    // After a transparent-boundary warp G'MIC's image has an (opaque) alpha channel, which
    // takes part in the maximum: arg1 = 255 then, else 0.
    let m = max(red_max(u32(arg(0u))), arg(1u));
    textureStore(dst, p, vec4<f32>(m - textureLoad(src, p, 0).rgb, 255.0));
}
"#;

/// RGB → HSX8, scale/shift, HSX8 → RGB (G'MIC formulas, no clamping).
/// arg: 0 mode (0 HSV, 1 HSI, 2 HSL), 1 hf, 2 hoff, 3 sf, 4 soff, 5 xf, 6 xoff
const HSX: &str = r#"
fn gmod(a: f32, b: f32) -> f32 { return a - b * floor(a / b); }

fn hue(c: vec3<f32>, M: f32, C: f32) -> f32 {
    if (C == 0.0) { return 0.0; }
    if (M == c.r) { return 60.0 * gmod((c.g - c.b) / C, 6.0); }
    if (M == c.g) { return 60.0 * ((c.b - c.r) / C + 2.0); }
    return 60.0 * ((c.r - c.g) / C + 4.0);
}

fn sector(h6: f32, C: f32, X: f32) -> vec3<f32> {
    let i = i32(h6);
    if (i == 0) { return vec3<f32>(C, X, 0.0); }
    if (i == 1) { return vec3<f32>(X, C, 0.0); }
    if (i == 2) { return vec3<f32>(0.0, C, X); }
    if (i == 3) { return vec3<f32>(0.0, X, C); }
    if (i == 4) { return vec3<f32>(X, 0.0, C); }
    return vec3<f32>(C, 0.0, X);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let mode = i32(arg(0u));
    let c = textureLoad(src, p, 0).rgb;
    let m = min(c.r, min(c.g, c.b));
    let M = max(c.r, max(c.g, c.b));
    let C = M - m;
    var h = hue(c, M, C);
    var s = 0.0;
    var x = 0.0;
    if (mode == 1) {
        let sum = c.r + c.g + c.b;
        s = select(1.0 - 3.0 * m / sum, 0.0, sum <= 0.0);
        x = sum / (3.0 * 255.0);
    } else if (mode == 2) {
        x = 0.5 * (m + M) / 255.0;
        s = select(C / (1.0 - abs(2.0 * x - 1.0)) / 255.0, 0.0, x == 1.0 || x == 0.0);
    } else {
        s = select(C / M, 0.0, M <= 0.0);
        x = M / 255.0;
    }
    // To 8-bit ranges, mix, back.
    var h8 = h * 0.708333;
    var s8 = s * 255.0;
    var x8 = x * 255.0;
    h8 = gmod(h8 * arg(1u) + arg(2u) * max(1.0, abs(arg(1u))), 255.0);
    s8 = s8 * arg(3u) + arg(4u) * max(1.0, abs(arg(3u)));
    x8 = x8 * arg(5u) + arg(6u) * max(1.0, abs(arg(5u)));
    let H = gmod((h8 / 0.708333) / 60.0, 6.0);
    let S = s8 / 255.0;
    let V = x8 / 255.0;
    var rgb = vec3<f32>(0.0);
    let hm = gmod(H, 2.0);
    if (mode == 1) {
        let Z = 1.0 - abs(hm - 1.0);
        let Cc = V * S / (1.0 + Z);
        rgb = (sector(H, Cc, Cc * Z) + V * (1.0 - S) / 3.0) * 3.0 * 255.0;
    } else if (mode == 2) {
        let Cc = (1.0 - abs(2.0 * V - 1.0)) * S;
        rgb = (sector(H, Cc, Cc * (1.0 - abs(hm - 1.0))) + (V - Cc / 2.0)) * 255.0;
    } else {
        let Cc = V * S;
        rgb = (sector(H, Cc, Cc * (1.0 - abs(hm - 1.0))) + (V - Cc)) * 255.0;
    }
    textureStore(dst, p, vec4<f32>(rgb, 255.0));
}
"#;

impl GpuEffect for UltraWarp2 {
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        let a = |i: usize, d: f64| arg(args, i, d);
        let iterations = a(1, 3.0).round().clamp(1.0, 30.0) as usize;
        let (xfc, xfr, yfc, yfr) = (a(3, 0.0), a(4, 3.0), a(5, 0.0), a(6, 3.0));
        let (xoffc, xoffr, yoffc, yoffr) = (a(7, 0.0), a(8, 3.0), a(9, 0.0), a(10, 3.0));
        let (corr, inter, bound) = (a(11, 2.0).round(), a(12, 2.0).round(), a(13, 5.0).round());
        let chann = a(14, 3.0).round() as i64;
        let random_negation = a(17, 1.0) >= 0.5;
        let every_iteration = a(18, 1.0) >= 0.5;
        let mode = a(19, 0.0).round().clamp(0.0, 2.0);
        let (hfc, hfr, sfc, sfr, vfc, vfr) = (a(20, 1.0), a(21, 0.5), a(22, 2.5), a(23, 5.0), a(24, 1.0), a(25, 0.0));
        let (hoc, hor, soc, sor, voc, vor) = (a(28, 0.0), a(29, 255.0), a(30, 0.0), a(31, 255.0), a(32, 0.0), a(33, 0.0));

        let (w, h) = (chain.width() as f64, chain.height() as f64);
        let mut rng = Rng(0xC0FFEE ^ (chain.seed as u64).wrapping_mul(0x9E37_79B9));
        let sq = |v: f64, dim: f64| v.signum() * dim * 0.00002 * v * v;

        // Warp channel mask (R=1, G=2, B=4) and whether a single channel is processed.
        let (mask, single) = match chann {
            4 => (1, true),
            5 => (2, true),
            6 => (4, true),
            _ => (7, false),
        };

        let mut has_alpha = false;
        for it in 0..iterations {
            if chann != 0 {
                let correl = if corr == 2.0 { rng.next().round() } else { corr };
                let interp = if inter == 2.0 { rng.next().round() } else { inter };
                let bounds = match bound as i64 {
                    4 => (rng.next() * 4.0).floor(),
                    5 => (rng.next() * 3.0).floor() + 1.0,
                    b => b as f64,
                };
                has_alpha |= bounds == 0.0;
                let xf = xfc + rng.u(-xfr, xfr);
                let yf = yfc + rng.u(-yfr, yfr);
                let xoff = xoffc + rng.u(-xoffr, xoffr);
                let yoff = yoffc + rng.u(-yoffr, yoffr);
                let mut p = chain.params();
                p.set(0, sq(xf, w))
                    .set(1, sq(yf, h))
                    .set(2, sq(xoff, w))
                    .set(3, sq(yoff, w)) // G'MIC uses the width here too
                    .set(4, if correl != 0.0 { 1.0 } else { 0.0 })
                    .set(5, interp)
                    .set(6, bounds)
                    .set(7, mask as f64)
                    .set(8, if single { 1.0 } else { 0.0 });
                chain.pass("ultrawarp_warp", WARP, &p);
            }
            if random_negation && rng.next().round() != 0.0 {
                let slot = (2 * it as u32).min(60);
                chain.reduce_minmax(slot);
                let mut p = chain.params();
                p.set(0, slot as f64).set(1, if has_alpha { 255.0 } else { 0.0 });
                chain.pass("ultrawarp_negate", NEGATE, &p);
            }
            if it == iterations - 1 || every_iteration {
                let hf = hfc + rng.u(-hfr, hfr);
                let sf = sfc + rng.u(-sfr, sfr);
                let vf = vfc + rng.u(-vfr, vfr);
                let hoff = hoc + rng.u(-hor, hor);
                let soff = soc + rng.u(-sor, sor);
                let voff = voc + rng.u(-vor, vor);
                let mut p = chain.params();
                p.set(0, mode).set(1, hf).set(2, hoff).set(3, sf).set(4, soff).set(5, vf).set(6, voff);
                chain.pass("ultrawarp_hsx", HSX, &p);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// UltraWarp 3 (`fx_ultrawarp3_preview` → `fx_ultrawarp3 ${2-16}`)
// ---------------------------------------------------------------------------------------
//
// Each iteration warps the image by its own intensity with a random magnitude/angle:
// `J(dx = v*wx - ox, dy = v*wy - oy)` with bicubic interpolation and mirror boundary (G'MIC
// hard-codes both). A fixed "Random Seed" (≠ -1) gives the same warp on every frame, like G'MIC.
//
// Arguments (`args[k]` = `fx_ultrawarp3 $k`): 1 seed · 2 iterations · 3/4 warp magnitude
// (centre, range) · 5/6 warp angle · 7/8 offset magnitude · 9/10 offset angle · 11 correlated ·
// 12 interpolation (unused by G'MIC) · 13 boundary (unused) · 14 colour space · 15 channels.

pub fn supports3(args: &[f64]) -> bool {
    arg(args, 14, 0.0).round() as i64 == 0 && (0..=5).contains(&(arg(args, 15, 0.0).round() as i64))
}

#[derive(Default)]
pub struct UltraWarp3;

/// arg: 0 wx, 1 wy, 2 ox, 3 oy, 4 correlated, 5 channel mask
const WARP3: &str = r#"
fn mirror_c(x: f32, n: f32) -> f32 {
    let m = x - 2.0 * n * floor(x / (2.0 * n));
    return select(m, 2.0 * n - m - 1.0, m >= n);
}

// CImg cubic (Catmull-Rom) interpolation, mirror boundary.
fn cubic(x0: f32, y0: f32) -> vec4<f32> {
    let s = size_f();
    let x = mirror_c(x0, s.x);
    let y = mirror_c(y0, s.y);
    let ix = i32(floor(x));
    let iy = i32(floor(y));
    let dx = x - floor(x);
    let dy = y - floor(y);
    var rows: array<vec4<f32>, 4>;
    for (var j = -1; j <= 2; j = j + 1) {
        let a = fetch(src, vec2<i32>(ix - 1, iy + j), 3);
        let b = fetch(src, vec2<i32>(ix, iy + j), 3);
        let c = fetch(src, vec2<i32>(ix + 1, iy + j), 3);
        let d = fetch(src, vec2<i32>(ix + 2, iy + j), 3);
        rows[j + 1] = b + 0.5 * (dx * (c - a) + dx * dx * (2.0 * a - 5.0 * b + 4.0 * c - d) + dx * dx * dx * (-a + 3.0 * b - 3.0 * c + d));
    }
    let a = rows[0];
    let b = rows[1];
    let c = rows[2];
    let d = rows[3];
    return b + 0.5 * (dy * (c - a) + dy * dy * (2.0 * a - 5.0 * b + 4.0 * c - d) + dy * dy * dy * (-a + 3.0 * b - 3.0 * c + d));
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let wx = arg(0u);
    let wy = arg(1u);
    let ox = arg(2u);
    let oy = arg(3u);
    let mask = u32(arg(5u));
    let c = textureLoad(src, p, 0).rgb;
    let fp = vec2<f32>(p);
    var out = c;
    if (arg(4u) > 0.5 && mask == 7u) {
        let ni = length(c);
        out = cubic(fp.x + ni * wx - ox, fp.y + ni * wy - oy).rgb;
    } else {
        for (var ch = 0u; ch < 3u; ch = ch + 1u) {
            if (((mask >> ch) & 1u) == 0u) { continue; }
            let v = select(c[ch], abs(c[ch]), arg(4u) > 0.5);
            out[ch] = cubic(fp.x + v * wx - ox, fp.y + v * wy - oy)[ch];
        }
    }
    textureStore(dst, p, vec4<f32>(out, 255.0));
}
"#;

impl GpuEffect for UltraWarp3 {
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        use std::f64::consts::PI;
        let a = |i: usize, d: f64| arg(args, i, d);
        let seed = a(1, -1.0).round();
        let iterations = a(2, 1.0).round().clamp(1.0, 10.0) as usize;
        let (wm, wmr) = (a(3, 1.0), a(4, 1.0));
        let (wa, war) = (a(5, 0.0) * PI / 180.0, a(6, 360.0) * PI / 180.0);
        let (w, h) = (chain.width() as f64, chain.height() as f64);
        let scale = (w * w + h * h).sqrt() * 0.001;
        let (om, omr) = (a(7, 1.0) * scale, a(8, 1.0) * scale);
        let (oa, oar) = (a(9, 0.0) * PI / 180.0, a(10, 360.0) * PI / 180.0);
        let correlated = a(11, 0.0) >= 0.5;
        let mask = match a(15, 0.0).round() as i64 {
            3 => 1,
            4 => 2,
            5 => 4,
            _ => 7,
        };
        let s = if seed >= 0.0 { seed as u64 } else { chain.seed as u64 };
        let mut rng = Rng(0x5EED_0003 ^ s.wrapping_mul(0x9E37_79B9));
        let tau = 2.0 * PI;
        let gmod = |x: f64, m: f64| x - m * (x / m).floor();
        for _ in 0..iterations {
            let wmag = ((wm + (rng.next() - 0.5) * wmr) * 0.1).powi(2) * 10.0;
            let omag = ((om + (rng.next() - 0.5) * omr) * 0.1).powi(2) * 10.0;
            let wangle = wa + (rng.next() - 0.5) * war;
            let oangle = oa + (rng.next() - 0.5) * oar;
            let mut p = chain.params();
            p.set(0, wmag * gmod(-wangle, tau).sin())
                .set(1, wmag * gmod(-wangle, tau).cos())
                .set(2, omag * gmod(-oangle, tau).sin())
                .set(3, omag * gmod(-oangle, tau).cos())
                .set(4, if correlated { 1.0 } else { 0.0 })
                .set(5, mask as f64);
            chain.pass("ultrawarp3_warp", WARP3, &p);
        }
        Ok(())
    }
}
