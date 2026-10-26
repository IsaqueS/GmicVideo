//! `fx_vhs_filter chroma_shift,noise,chroma_blur,luma_blur,sharpen`
//!
//! G'MIC: shift the red channel by `chroma_shift` px (zero boundary) · gaussian noise ·
//! RGB→YIQ · blur I,Q along x by `chroma_blur` and Y along x by `luma_blur` · YIQ→RGB (clamped)
//! · `unsharp luma_blur,sharpen` · clamp.

use super::{arg, gaussian_blur, GpuEffect};
use crate::gpu::PassChain;

pub fn supports(_args: &[f64]) -> bool {
    true
}

pub struct Vhs;

/// Red shift + noise + RGB→YIQ. arg0 shift, arg1 noise sigma.
const PRE: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let c = textureLoad(src, p, 0).rgb;
    let r = fetch(src, p - vec2<i32>(i32(round(arg(0u))), 0), 0).r;
    var v = vec3<f32>(r, c.g, c.b);
    let s = arg(1u);
    if (s > 0.0) { v = v + s * vec3<f32>(randn(p, 0u), randn(p, 1u), randn(p, 2u)); }
    let y = dot(v, vec3<f32>(0.299, 0.587, 0.114));
    let i = dot(v, vec3<f32>(0.595716, -0.274453, -0.321263));
    let q = dot(v, vec3<f32>(0.211456, -0.522591, 0.311135));
    textureStore(dst, p, vec4<f32>(y, i, q, 255.0));
}
"#;

/// Horizontal gaussian blur with one sigma for Y (arg0) and one for I,Q (arg1); Neumann.
const BLUR_X: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let sy = arg(0u);
    let sc = arg(1u);
    let r = i32(ceil(3.0 * max(sy, sc)));
    var acc = vec3<f32>(0.0);
    var ws = vec3<f32>(0.0);
    for (var i = -r; i <= r; i = i + 1) {
        let f = f32(i * i);
        let wy = select(select(0.0, 1.0, i == 0), exp(-f / (2.0 * sy * sy)), sy >= 0.2);
        let wc = select(select(0.0, 1.0, i == 0), exp(-f / (2.0 * sc * sc)), sc >= 0.2);
        let w = vec3<f32>(wy, wc, wc);
        acc = acc + w * fetch(src, p + vec2<i32>(i, 0), 1).rgb;
        ws = ws + w;
    }
    textureStore(dst, p, vec4<f32>(acc / ws, 255.0));
}
"#;

const YIQ2RGB: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let c = textureLoad(src, p, 0).rgb;
    let r = dot(c, vec3<f32>(1.0, 0.9563, 0.6210));
    let g = dot(c, vec3<f32>(1.0, -0.2721, -0.6474));
    let b = dot(c, vec3<f32>(1.0, -1.1070, 1.7046));
    textureStore(dst, p, vec4<f32>(clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(255.0)), 255.0));
}
"#;

/// I - (B - I) * amount, clamped; src = B, aux = I.
const UNSHARP: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let b = textureLoad(src, p, 0).rgb;
    let i = textureLoad(aux, p, 0).rgb;
    textureStore(dst, p, vec4<f32>(clamp(i - (b - i) * arg(0u), vec3<f32>(0.0), vec3<f32>(255.0)), 255.0));
}
"#;

impl GpuEffect for Vhs {
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        let shift = arg(args, 0, 3.0);
        let noise = arg(args, 1, 15.0).max(0.0);
        let chroma = arg(args, 2, 20.0).max(0.0);
        let luma = arg(args, 3, 5.0).max(0.0);
        let sharpen = arg(args, 4, 1.0);

        let mut p = chain.params();
        p.set(0, shift).set(1, noise);
        chain.pass("vhs_pre", PRE, &p);
        let mut p = chain.params();
        p.set(0, luma).set(1, chroma);
        chain.pass("vhs_blur_x", BLUR_X, &p);
        let p = chain.params();
        chain.pass("vhs_yiq2rgb", YIQ2RGB, &p);
        if sharpen != 0.0 {
            let img = chain.save(0);
            gaussian_blur(chain, luma, luma, 1);
            let mut p = chain.params();
            p.set(0, sharpen);
            chain.pass_with_aux("vhs_unsharp", UNSHARP, &p, Some(&img));
        }
        Ok(())
    }
}
