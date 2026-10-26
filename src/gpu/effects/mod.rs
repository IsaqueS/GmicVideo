//! GPU versions of G'MIC filters.
//!
//! Each effect reads the same numeric arguments G'MIC would receive (same order), so the
//! parameter UI, tweens and presets work unchanged. `supports` lets an effect decline
//! argument combinations it does not implement (e.g. exotic colour-space channel modes);
//! those fall back to the CPU (G'MIC) path.

pub mod blur;
pub mod halftone;
pub mod noise;
pub mod sharpen;
pub mod ultrawarp;
pub mod vhs;
pub mod vignette;

use super::{GpuContext, PassChain, Params};
use crate::frame::Frame;

pub trait GpuEffect: Send {
    /// CPU-side preparation with the source frame (before upload), e.g. histograms.
    fn prepare(&mut self, _ctx: &GpuContext, _frame: &Frame, _args: &[f64]) -> Result<(), String> {
        Ok(())
    }
    /// Records the compute passes for one frame.
    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String>;
    /// The full G'MIC command line and seed of the frame (for effects that run G'MIC
    /// themselves, like baked warps). Called before `prepare`.
    fn set_command_line(&mut self, _command_line: &str, _seed: u32) {}
}

pub struct GpuEffectInfo {
    pub name: &'static str,
    /// G'MIC commands this effect replaces (first token of the command line).
    pub commands: &'static [&'static str],
    pub supports: fn(&[f64]) -> bool,
    pub create: fn() -> Box<dyn GpuEffect>,
    /// Only valid while no parameter is animated (baked warps: re-baking every frame would
    /// cost as much as running G'MIC itself).
    pub static_only: bool,
}

pub fn registry() -> &'static [GpuEffectInfo] {
    static REGISTRY: &[GpuEffectInfo] = &[
        GpuEffectInfo {
            name: "Vignette",
            commands: &["fx_vignette"],
            supports: vignette::supports,
            create: || Box::new(vignette::Vignette),
            static_only: false,
        },
        GpuEffectInfo {
            name: "Blur [Gaussian]",
            commands: &["fx_blur_gaussian"],
            supports: blur::supports,
            create: || Box::new(blur::GaussianBlur),
            static_only: false,
        },
        GpuEffectInfo {
            name: "Halftone",
            commands: &["fx_halftone"],
            supports: halftone::supports,
            create: || Box::new(halftone::Halftone::default()),
            static_only: false,
        },
        GpuEffectInfo {
            name: "Sharpen [Unsharp Mask]",
            commands: &["fx_unsharp"],
            supports: sharpen::supports,
            create: || Box::new(sharpen::UnsharpMask),
            static_only: false,
        },
        GpuEffectInfo {
            name: "Noise [Additive]",
            commands: &["fx_noise"],
            supports: noise::supports,
            create: || Box::new(noise::Noise),
            static_only: false,
        },
        GpuEffectInfo {
            name: "VHS Filter",
            commands: &["fx_vhs_filter"],
            supports: vhs::supports,
            create: || Box::new(vhs::Vhs),
            static_only: false,
        },
        GpuEffectInfo {
            name: "UltraWarp 2",
            commands: &["fx_ultrawarptwo"],
            supports: ultrawarp::supports,
            create: || Box::new(ultrawarp::UltraWarp2::default()),
            static_only: false,
        },
        GpuEffectInfo {
            name: "UltraWarp 3",
            commands: &["fx_ultrawarp3_preview", "fx_ultrawarp3"],
            supports: ultrawarp::supports3,
            create: || Box::new(ultrawarp::UltraWarp3::default()),
            static_only: false,
        },
    ];
    REGISTRY
}

/// The GPU effect for a G'MIC command: hand-written ports first, then baked warps.
pub fn find(command: &str) -> Option<&'static GpuEffectInfo> {
    registry()
        .iter()
        .chain(crate::gpu::warp::registry())
        .find(|e| e.commands.contains(&command))
}

/// Argument `i` or `default` when missing / not a number.
pub fn arg(args: &[f64], i: usize, default: f64) -> f64 {
    args.get(i).copied().filter(|v| v.is_finite()).unwrap_or(default)
}

// ------------------------------------------------------------------------------------------
// Shared building blocks
// ------------------------------------------------------------------------------------------

/// Colour-space conversions used by G'MIC's `apply_channels` (values in 0..255).
pub const COLOR_WGSL: &str = r#"
fn srgb2lin1(v: f32) -> f32 {
    let x = clamp(v / 255.0, 0.0, 1.0);
    return 255.0 * select(pow((x + 0.055) / 1.055, 2.4), x / 12.92, x <= 0.04045);
}
fn lin2srgb1(v: f32) -> f32 {
    let x = clamp(v / 255.0, 0.0, 1.0);
    return 255.0 * select(1.055 * pow(x, 1.0 / 2.4) - 0.055, 12.92 * x, x <= 0.0031308);
}
fn srgb2lin(c: vec3<f32>) -> vec3<f32> { return vec3<f32>(srgb2lin1(c.r), srgb2lin1(c.g), srgb2lin1(c.b)); }
fn lin2srgb(c: vec3<f32>) -> vec3<f32> { return vec3<f32>(lin2srgb1(c.r), lin2srgb1(c.g), lin2srgb1(c.b)); }
"#;

/// `apply_channels` modes the GPU effects understand: 0 = all, 1 = RGBA all, 2 = RGB, 7 = linear RGB.
pub fn channels_supported(mode: f64) -> bool {
    matches!(mode.round() as i64, 0 | 1 | 2 | 7)
}

/// Pass converting the current image to/from linear RGB (`arg(0)`: 0 = to linear, 1 = to sRGB).
pub fn linear_rgb_pass(chain: &mut PassChain, to_linear: bool) {
    const BODY: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let c = textureLoad(src, p, 0);
    let rgb = select(lin2srgb(c.rgb), srgb2lin(c.rgb), arg(0u) < 0.5);
    textureStore(dst, p, vec4<f32>(rgb, c.a));
}
"#;
    let key = "linear_rgb";
    let body = format!("{COLOR_WGSL}\n{BODY}");
    let mut params = chain.params();
    params.set(0, if to_linear { 0.0 } else { 1.0 });
    chain.pass(key, &body, &params);
}

/// Separable gaussian blur (G'MIC `blur sigma,boundary`), applied as two passes.
pub fn gaussian_blur(chain: &mut PassChain, sigma_x: f64, sigma_y: f64, boundary: i32) {
    const BODY: &str = r#"
// arg0 = sigma, arg1 = boundary, arg2 = axis (0 = x, 1 = y)
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let sigma = arg(0u);
    let boundary = i32(arg(1u));
    let dir = select(vec2<i32>(1, 0), vec2<i32>(0, 1), arg(2u) > 0.5);
    let radius = i32(ceil(3.0 * sigma));
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = -radius; i <= radius; i = i + 1) {
        let w = exp(-f32(i * i) / (2.0 * sigma * sigma));
        acc = acc + w * fetch(src, p + dir * i, boundary);
        wsum = wsum + w;
    }
    textureStore(dst, p, acc / wsum);
}
"#;
    for (axis, sigma) in [(0.0, sigma_x), (1.0, sigma_y)] {
        if sigma < 0.2 {
            continue;
        }
        let mut params = chain.params();
        params.set(0, sigma).set(1, boundary as f64).set(2, axis);
        chain.pass("gaussian_blur", BODY, &params);
    }
}

/// Parameter block helper for effects with many values.
pub fn params_from(chain: &PassChain, values: &[f64]) -> Params {
    chain.params().with(values)
}
