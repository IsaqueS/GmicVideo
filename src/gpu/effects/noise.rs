//! `fx_noise amplitude,type,channels,value_action` — additive noise (G'MIC `noise`).
//!
//! Types: 0 gaussian (σ = amplitude), 1 uniform (±amplitude), 2 salt & pepper (amplitude % of
//! values become the image min or max), 3 poisson. The random numbers come from a per-pixel
//! hash seeded by the frame index, so they differ from G'MIC's generator but have the same
//! distribution (and are repeatable per frame, like the CPU path).

use super::{arg, channels_supported, linear_rgb_pass, GpuEffect};
use crate::gpu::PassChain;

pub fn supports(args: &[f64]) -> bool {
    channels_supported(arg(args, 2, 7.0)) && arg(args, 3, 1.0).round() as i64 != 2
}

pub struct Noise;

const BODY: &str = r#"
// arg0 amplitude, arg1 type; reduction slot 0 = image min/max (salt & pepper)
fn poisson(lambda: f32, p: vec2<i32>, s: u32) -> f32 {
    if (lambda > 30.0) { return max(0.0, round(lambda + sqrt(lambda) * randn(p, s))); }
    // Knuth for small means.
    let l = exp(-lambda);
    var k = 0.0;
    var prod = rand01(p, s * 7u + 11u);
    for (var i = 0u; i < 200u; i = i + 1u) {
        if (prod <= l) { break; }
        k = k + 1.0;
        prod = prod * rand01(p, s * 7u + 12u + i);
    }
    return k;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let a = arg(0u);
    let t = i32(arg(1u));
    var c = textureLoad(src, p, 0).rgb;
    let m = red_min(0u);
    let M = red_max(0u);
    for (var ch = 0u; ch < 3u; ch = ch + 1u) {
        if (t == 0) {
            c[ch] = c[ch] + a * randn(p, ch);
        } else if (t == 1) {
            c[ch] = c[ch] + a * (2.0 * rand01(p, ch) - 1.0);
        } else if (t == 2) {
            if (rand01(p, ch + 10u) * 100.0 < a) {
                c[ch] = select(m, M, rand01(p, ch + 20u) < 0.5);
            }
        } else {
            c[ch] = poisson(max(c[ch], 0.0), p, ch);
        }
    }
    textureStore(dst, p, vec4<f32>(c, 255.0));
}
"#;

impl GpuEffect for Noise {
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        let amplitude = arg(args, 0, 10.0).max(0.0);
        let kind = arg(args, 1, 0.0).round().clamp(0.0, 3.0);
        let linear = arg(args, 2, 7.0).round() as i64 == 7;
        let cut = arg(args, 3, 1.0).round() as i64 == 1;

        if linear {
            linear_rgb_pass(chain, true);
        }
        if kind == 2.0 {
            chain.reduce_minmax(0);
        }
        let mut p = chain.params();
        p.set(0, amplitude).set(1, kind);
        chain.pass("noise", BODY, &p);
        if linear {
            // Back-conversion clamps to [0,255], like `value_action = cut`; without cut the final
            // store clamps anyway since frames are 8-bit.
            let _ = cut;
            linear_rgb_pass(chain, false);
        }
        Ok(())
    }
}
