//! `fx_unsharp type,radius,bilateral_radius,amount,threshold,dark,light,iterations,negative,channels`
//!
//! G'MIC (`_fx_unsharp`, per iteration):
//! `B = blur(I)` (gaussian or bilateral) · `D = (I - B) * amount` · keep D where
//! `norm(D) >= threshold%` of its range · optional negation · `I += min(D,0)*dark + max(D,0)*light`
//! · clamp to [0,255]. CImg's bilateral filter is per channel; here it is computed exactly
//! (G'MIC uses a downsampled bilateral grid, so results differ very slightly).

use super::{arg, channels_supported, gaussian_blur, linear_rgb_pass, GpuEffect};
use crate::gpu::PassChain;

pub fn supports(args: &[f64]) -> bool {
    channels_supported(arg(args, 9, 0.0))
}

pub struct UnsharpMask;

const BILATERAL: &str = r#"
// arg0 sigma_s, arg1 sigma_r — exact per-channel bilateral filter, Neumann boundary.
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let ss = max(arg(0u), 0.01);
    let sr = max(arg(1u), 0.01);
    let r = i32(ceil(3.0 * ss));
    let c0 = textureLoad(src, p, 0).rgb;
    var acc = vec3<f32>(0.0);
    var wsum = vec3<f32>(0.0);
    for (var j = -r; j <= r; j = j + 1) {
        for (var i = -r; i <= r; i = i + 1) {
            let ws = exp(-f32(i * i + j * j) / (2.0 * ss * ss));
            let c = fetch(src, p + vec2<i32>(i, j), 1).rgb;
            let d = c - c0;
            let w = ws * exp(-(d * d) / (2.0 * sr * sr));
            acc = acc + w * c;
            wsum = wsum + w;
        }
    }
    textureStore(dst, p, vec4<f32>(acc / wsum, 255.0));
}
"#;

/// D = (I - B) * amount; src = B, aux = I.
const DIFF: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let b = textureLoad(src, p, 0).rgb;
    let i = textureLoad(aux, p, 0).rgb;
    textureStore(dst, p, vec4<f32>((i - b) * arg(0u), 255.0));
}
"#;

/// Writes |D| (euclidean norm over channels) into all channels.
const NORM: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let n = length(textureLoad(src, p, 0).rgb);
    textureStore(dst, p, vec4<f32>(n, n, n, 255.0));
}
"#;

/// Keeps D (aux) where norm (src) >= threshold% of the norm range (reduction slot arg1).
const MASK: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let m = red_min(u32(arg(1u)));
    let M = red_max(u32(arg(1u)));
    let thr = m + arg(0u) / 100.0 * (M - m);
    let n = textureLoad(src, p, 0).r;
    let d = textureLoad(aux, p, 0).rgb;
    textureStore(dst, p, vec4<f32>(select(vec3<f32>(0.0), d, n >= thr), 255.0));
}
"#;

/// I + min(D,0)*dark + max(D,0)*light; src = D, aux = I. arg: 0 dark, 1 light, 2 negative.
const APPLY: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    var d = textureLoad(src, p, 0).rgb;
    if (arg(2u) > 0.5) { d = -d; }
    let i = textureLoad(aux, p, 0).rgb;
    let v = i + min(d, vec3<f32>(0.0)) * arg(0u) + max(d, vec3<f32>(0.0)) * arg(1u);
    textureStore(dst, p, vec4<f32>(clamp(v, vec3<f32>(0.0), vec3<f32>(255.0)), 255.0));
}
"#;

impl GpuEffect for UnsharpMask {
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        let bilateral = arg(args, 0, 1.0) >= 0.5;
        let radius = arg(args, 1, 1.25).max(0.0);
        let range = arg(args, 2, 10.0).max(0.0);
        let amount = arg(args, 3, 2.0);
        let threshold = arg(args, 4, 0.0).max(0.0);
        let (dark, light) = (arg(args, 5, 1.0), arg(args, 6, 1.0));
        let iterations = arg(args, 7, 1.0).round().clamp(1.0, 10.0) as usize;
        let negative = arg(args, 8, 0.0) >= 0.5;
        let linear = arg(args, 9, 0.0).round() as i64 == 7;

        if linear {
            linear_rgb_pass(chain, true);
        }
        for it in 0..iterations {
            let slot = (2 * it as u32).min(60);
            let original = chain.save(0);
            if bilateral {
                let mut p = chain.params();
                p.set(0, radius).set(1, range);
                chain.pass("unsharp_bilateral", BILATERAL, &p);
            } else {
                gaussian_blur(chain, radius, radius, 1);
            }
            let mut p = chain.params();
            p.set(0, amount);
            chain.pass_with_aux("unsharp_diff", DIFF, &p, Some(&original));
            if threshold > 0.0 {
                let diff = chain.save(1);
                let p = chain.params();
                chain.pass("unsharp_norm", NORM, &p);
                chain.reduce_minmax(slot);
                let mut p = chain.params();
                p.set(0, threshold).set(1, slot as f64);
                chain.pass_with_aux("unsharp_mask", MASK, &p, Some(&diff));
            }
            let mut p = chain.params();
            p.set(0, dark).set(1, light).set(2, if negative { 1.0 } else { 0.0 });
            chain.pass_with_aux("unsharp_apply", APPLY, &p, Some(&original));
        }
        if linear {
            linear_rgb_pass(chain, false);
        }
        Ok(())
    }
}
