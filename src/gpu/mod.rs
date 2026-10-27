//! Vulkan GPU effects (wgpu forced to the Vulkan backend).
//!
//! G'MIC itself only runs on the CPU, so selected effects are re-implemented as compute
//! shaders that read the same G'MIC arguments (in the same order), which keeps tweens,
//! curves and presets working unchanged. Effects without a GPU version keep using G'MIC.
//!
//! Frame flow for one frame:
//! ```text
//!  planar RGB (CPU) → RGBA8 texture → "load" pass → orig (rgba32f, values 0..255)
//!     → effect passes (ping-pong rgba32f) → "store" pass (round + clamp) → RGBA8 → readback
//! ```
//! Shaders work with values in 0..255, like G'MIC, so formulas port directly.

pub mod effects;
pub mod processor;
pub mod warp;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use wgpu::util::DeviceExt;

pub const WG: u32 = 16;

/// Uniform block shared by every pass (must match `PRELUDE`).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Params {
    /// width, height, 1/width, 1/height
    pub size: [f32; 4],
    /// seed, frame index, pass index, free
    pub misc: [f32; 4],
    /// 128 effect-specific values (`arg(i)` in WGSL)
    pub a: [[f32; 4]; 32],
}

impl Params {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            size: [w as f32, h as f32, 1.0 / w.max(1) as f32, 1.0 / h.max(1) as f32],
            misc: [0.0; 4],
            a: [[0.0; 4]; 32],
        }
    }

    pub fn set(&mut self, i: usize, v: f64) -> &mut Self {
        self.a[i / 4][i % 4] = v as f32;
        self
    }

    #[allow(dead_code)]
    pub fn with(mut self, values: &[f64]) -> Self {
        for (i, v) in values.iter().take(128).enumerate() {
            self.set(i, *v);
        }
        self
    }

    pub fn seed(mut self, seed: u32) -> Self {
        self.misc[0] = (seed % 16_777_216) as f32;
        self
    }
}

/// WGSL shared by all effect shaders.
pub const PRELUDE: &str = r#"
struct Params {
    size: vec4<f32>,
    misc: vec4<f32>,
    a: array<vec4<f32>, 32>,
};
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var dst: texture_storage_2d<rgba32float, write>;
@group(0) @binding(3) var orig: texture_2d<f32>;
@group(0) @binding(4) var<uniform> P: Params;
@group(0) @binding(5) var aux: texture_2d<f32>;
// Per-frame reduction slots (cleared to 0 before each frame), see `PassChain::reduce_minmax`.
@group(0) @binding(6) var<storage, read_write> R: array<atomic<u32>, 64>;

// Order-preserving u32 keys for floats (so atomicMax works on any sign). Minima are stored
// as the bitwise NOT of the key, so both slots start at 0 and only need atomicMax.
fn red_key(f: f32) -> u32 {
    let b = bitcast<u32>(f);
    return select(b | 0x80000000u, ~b, (b & 0x80000000u) != 0u);
}
fn red_val(k: u32) -> f32 {
    return bitcast<f32>(select(~k, k & 0x7fffffffu, (k & 0x80000000u) != 0u));
}
fn red_max(slot: u32) -> f32 { return red_val(atomicLoad(&R[slot])); }
fn red_min(slot: u32) -> f32 { return red_val(~atomicLoad(&R[slot + 1u])); }

fn arg(i: u32) -> f32 { return P.a[i / 4u][i % 4u]; }
fn size_f() -> vec2<f32> { return P.size.xy; }
fn size_i() -> vec2<i32> { return vec2<i32>(P.size.xy); }
fn seed() -> f32 { return P.misc.x; }

// G'MIC boundary conditions: 0 = dirichlet (zero), 1 = neumann (nearest), 2 = periodic, 3 = mirror.
fn wrap_coord(c: i32, n: i32, boundary: i32) -> i32 {
    if (boundary == 2) { return ((c % n) + n) % n; }
    if (boundary == 3) {
        let m = 2 * n;
        let r = ((c % m) + m) % m;
        return select(r, m - 1 - r, r >= n);
    }
    return clamp(c, 0, n - 1);
}

fn fetch(t: texture_2d<f32>, p: vec2<i32>, boundary: i32) -> vec4<f32> {
    let s = vec2<i32>(textureDimensions(t));
    if (boundary == 0 && (p.x < 0 || p.y < 0 || p.x >= s.x || p.y >= s.y)) { return vec4<f32>(0.0); }
    return textureLoad(t, vec2<i32>(wrap_coord(p.x, s.x, boundary), wrap_coord(p.y, s.y, boundary)), 0);
}

// Bilinear interpolation at pixel coordinates (pixel centers are integers, like G'MIC).
fn bilinear(t: texture_2d<f32>, x: f32, y: f32, boundary: i32) -> vec4<f32> {
    let fx = floor(x);
    let fy = floor(y);
    let dx = x - fx;
    let dy = y - fy;
    let ix = i32(fx);
    let iy = i32(fy);
    let a = fetch(t, vec2<i32>(ix, iy), boundary);
    let b = fetch(t, vec2<i32>(ix + 1, iy), boundary);
    let c = fetch(t, vec2<i32>(ix, iy + 1), boundary);
    let d = fetch(t, vec2<i32>(ix + 1, iy + 1), boundary);
    return mix(mix(a, b, dx), mix(c, d, dx), dy);
}

fn nearest(t: texture_2d<f32>, x: f32, y: f32, boundary: i32) -> vec4<f32> {
    return fetch(t, vec2<i32>(i32(round(x)), i32(round(y))), boundary);
}

fn luma(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.22248840, 0.71690369, 0.06060791)); }

// Hash-based random numbers (deterministic per pixel, frame seed and stream).
fn hash_u32(v: u32) -> u32 {
    var x = v;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    x = x ^ (x >> 16u);
    return x;
}
fn rand01(p: vec2<i32>, stream: u32) -> f32 {
    let h = hash_u32(u32(p.x) * 73856093u ^ u32(p.y) * 19349663u ^ hash_u32(u32(seed()) + stream * 83492791u));
    return f32(h & 0x00ffffffu) / 16777216.0;
}
fn randn(p: vec2<i32>, stream: u32) -> f32 {
    let u1 = max(rand01(p, stream * 2u + 1u), 1e-7);
    let u2 = rand01(p, stream * 2u + 2u);
    return sqrt(-2.0 * log(u1)) * cos(6.28318530718 * u2);
}
"#;

const LOAD_SHADER: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    textureStore(dst, p, textureLoad(src, p, 0) * 255.0);
}
"#;

/// Writes rounded 0..1 values into the RGBA8 output (binding `dst` is rgba8unorm here).
const STORE_SHADER: &str = r#"
struct StoreParams { size: vec4<f32>, };
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> S: StoreParams;
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= i32(S.size.x) || p.y >= i32(S.size.y)) { return; }
    let v = round(clamp(textureLoad(src, p, 0), vec4<f32>(0.0), vec4<f32>(255.0)));
    textureStore(dst, p, vec4<f32>(v.rgb / 255.0, 1.0));
}
"#;

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    store_layout: wgpu::BindGroupLayout,
    store_pipeline: wgpu::ComputePipeline,
    sampler: wgpu::Sampler,
    dummy_aux: wgpu::TextureView,
    pipelines: Mutex<HashMap<String, Arc<wgpu::ComputePipeline>>>,
}

static GPU: OnceLock<Option<Arc<GpuContext>>> = OnceLock::new();

impl GpuContext {
    /// The shared GPU context, or `None` if no suitable Vulkan device exists
    /// (or `GMIC_VIDEO_DISABLE_GPU=1`). Software rasterizers (llvmpipe) are rejected unless
    /// `GMIC_VIDEO_FORCE_GPU=1`.
    pub fn get() -> Option<Arc<GpuContext>> {
        GPU.get_or_init(|| match Self::init() {
            Ok(ctx) => {
                eprintln!("[GPU] Using {} ({:?}, {})", ctx.info.name, ctx.info.device_type, ctx.info.driver);
                Some(Arc::new(ctx))
            }
            Err(e) => {
                eprintln!("[GPU] Not available, using CPU effects only: {e}");
                None
            }
        })
        .clone()
    }

    pub fn is_available() -> bool {
        Self::get().is_some()
    }

    fn init() -> Result<Self, String> {
        if std::env::var("GMIC_VIDEO_DISABLE_GPU").map(|v| v == "1").unwrap_or(false) {
            return Err("disabled by GMIC_VIDEO_DISABLE_GPU".into());
        }
        let force = std::env::var("GMIC_VIDEO_FORCE_GPU").map(|v| v == "1").unwrap_or(false);

        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(desc);

        let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
        let rank = |a: &wgpu::Adapter| match a.get_info().device_type {
            wgpu::DeviceType::DiscreteGpu => 0,
            wgpu::DeviceType::IntegratedGpu => 1,
            wgpu::DeviceType::VirtualGpu => 2,
            wgpu::DeviceType::Other => 3,
            wgpu::DeviceType::Cpu => 4,
        };
        let adapter = adapters
            .into_iter()
            .filter(|a| force || a.get_info().device_type != wgpu::DeviceType::Cpu)
            .min_by_key(rank)
            .ok_or("no Vulkan GPU found")?;
        let info = adapter.get_info();

        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gmicvideo"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits {
                max_texture_dimension_2d: limits.max_texture_dimension_2d,
                max_buffer_size: limits.max_buffer_size,
                ..wgpu::Limits::default()
            },
            ..Default::default()
        }))
        .map_err(|e| format!("request_device failed: {e}"))?;

        let tex_entry = |binding, filterable| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let uniform_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let storage_entry = |binding, format| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        };

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effect layout"),
            entries: &[
                tex_entry(0, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                storage_entry(2, wgpu::TextureFormat::Rgba32Float),
                tex_entry(3, false),
                uniform_entry(4),
                tex_entry(5, false),
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effect pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });

        let store_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("store layout"),
            entries: &[tex_entry(0, false), storage_entry(1, wgpu::TextureFormat::Rgba8Unorm), uniform_entry(2)],
        });
        let store_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("store pipeline layout"),
            bind_group_layouts: &[Some(&store_layout)],
            immediate_size: 0,
        });
        let store_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("store"),
            source: wgpu::ShaderSource::Wgsl(STORE_SHADER.into()),
        });
        let store_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("store"),
            layout: Some(&store_pl),
            module: &store_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let dummy = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dummy aux"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy_aux = dummy.create_view(&Default::default());

        Ok(Self {
            device,
            queue,
            info,
            layout,
            pipeline_layout,
            store_layout,
            store_pipeline,
            sampler,
            dummy_aux,
            pipelines: Mutex::new(HashMap::new()),
        })
    }

    /// Compiles (once) the effect shader `body` with the shared prelude.
    pub fn pipeline(&self, key: &str, body: &str) -> Arc<wgpu::ComputePipeline> {
        if let Some(p) = self.pipelines.lock().unwrap().get(key) {
            return p.clone();
        }
        let source = format!("{PRELUDE}\n{body}");
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(key),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = Arc::new(self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(key),
            layout: Some(&self.pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        }));
        self.pipelines.lock().unwrap().insert(key.to_string(), pipeline.clone());
        pipeline
    }

    pub fn texture(&self, w: u32, h: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages, label: &str) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }
}

/// GPU textures for one frame size (reused between frames).
pub struct Targets {
    pub width: u32,
    pub height: u32,
    input: wgpu::Texture,
    orig: wgpu::Texture,
    ping: [wgpu::Texture; 2],
    out8: wgpu::Texture,
    readback: wgpu::Buffer,
    padded_row: u32,
    /// Copies of intermediate images (`PassChain::save`).
    saved: [wgpu::Texture; 2],
    /// Reduction slots (`PassChain::reduce_minmax`).
    reduce: wgpu::Buffer,
}

impl Targets {
    pub fn new(ctx: &GpuContext, w: u32, h: u32) -> Self {
        use wgpu::TextureFormat as F;
        use wgpu::TextureUsages as U;
        let work = U::TEXTURE_BINDING | U::STORAGE_BINDING | U::COPY_SRC | U::COPY_DST;
        let padded_row = (w * 4).div_ceil(256) * 256;
        Self {
            width: w,
            height: h,
            input: ctx.texture(w, h, F::Rgba8Unorm, U::TEXTURE_BINDING | U::COPY_DST, "input"),
            orig: ctx.texture(w, h, F::Rgba32Float, work, "orig"),
            ping: [ctx.texture(w, h, F::Rgba32Float, work, "ping0"), ctx.texture(w, h, F::Rgba32Float, work, "ping1")],
            out8: ctx.texture(w, h, F::Rgba8Unorm, U::STORAGE_BINDING | U::COPY_SRC, "out8"),
            readback: ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: padded_row as u64 * h as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            padded_row,
            saved: [
                ctx.texture(w, h, F::Rgba32Float, U::TEXTURE_BINDING | U::COPY_DST, "saved0"),
                ctx.texture(w, h, F::Rgba32Float, U::TEXTURE_BINDING | U::COPY_DST, "saved1"),
            ],
            reduce: ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reduce"),
                size: 64 * 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
}

/// Which texture holds the current image.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Current {
    Orig,
    Ping(usize),
}

/// Records effect passes for one frame.
pub struct PassChain<'a> {
    pub ctx: &'a GpuContext,
    pub targets: &'a Targets,
    encoder: wgpu::CommandEncoder,
    current: Current,
    pub seed: u32,
}

impl<'a> PassChain<'a> {
    pub fn width(&self) -> u32 {
        self.targets.width
    }
    pub fn height(&self) -> u32 {
        self.targets.height
    }

    /// New parameter block for this frame size (seeded).
    pub fn params(&self) -> Params {
        Params::new(self.targets.width, self.targets.height).seed(self.seed)
    }

    fn view_of(&self, c: Current) -> wgpu::TextureView {
        match c {
            Current::Orig => self.targets.orig.create_view(&Default::default()),
            Current::Ping(i) => self.targets.ping[i].create_view(&Default::default()),
        }
    }

    /// Runs one compute pass reading the current image and writing the next one.
    pub fn pass(&mut self, key: &str, body: &str, params: &Params) {
        self.pass_with_aux(key, body, params, None);
    }

    /// Like `pass`, with an extra read-only texture bound as `aux` (rgba32float).
    pub fn pass_with_aux(&mut self, key: &str, body: &str, params: &Params, aux: Option<&wgpu::TextureView>) {
        let pipeline = self.ctx.pipeline(key, body);
        let next = match self.current {
            Current::Orig => Current::Ping(0),
            Current::Ping(i) => Current::Ping(1 - i),
        };
        let src = self.view_of(self.current);
        let dst = self.view_of(next);
        let orig = self.view_of(Current::Orig);
        let ubuf = self.ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind = self.ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(key),
            layout: &self.ctx.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.ctx.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&dst) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&orig) },
                wgpu::BindGroupEntry { binding: 4, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(aux.unwrap_or(&self.ctx.dummy_aux)),
                },
                wgpu::BindGroupEntry { binding: 6, resource: self.targets.reduce.as_entire_binding() },
            ],
        });
        {
            let mut cpass = self.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(key), timestamp_writes: None });
            cpass.set_pipeline(&pipeline);
            cpass.set_bind_group(0, &bind, &[]);
            cpass.dispatch_workgroups(self.targets.width.div_ceil(WG), self.targets.height.div_ceil(WG), 1);
        }
        self.current = next;
    }

    /// Copies the current image into saved slot `i` (0 or 1) and returns a view of it, for
    /// passes that need an earlier image (bind it as `aux`).
    pub fn save(&mut self, i: usize) -> wgpu::TextureView {
        let tex = &self.targets.saved[i];
        let src = match self.current {
            Current::Orig => &self.targets.orig,
            Current::Ping(i) => &self.targets.ping[i],
        };
        self.encoder.copy_texture_to_texture(
            src.as_image_copy(),
            tex.as_image_copy(),
            wgpu::Extent3d { width: self.targets.width, height: self.targets.height, depth_or_array_layers: 1 },
        );
        tex.create_view(&Default::default())
    }

    /// Computes the min and max over the R, G and B values of the current image into
    /// reduction slots `slot` (max) and `slot + 1` (min); read them in later passes with
    /// `red_max(slot)` / `red_min(slot)`. The image itself is unchanged. `slot` < 62.
    pub fn reduce_minmax(&mut self, slot: u32) {
        const BODY: &str = r#"
var<workgroup> wg_max: atomic<u32>;
var<workgroup> wg_min: atomic<u32>;
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let p = vec2<i32>(gid.xy);
    if (p.x < size_i().x && p.y < size_i().y) {
        let c = textureLoad(src, p, 0);
        for (var k = 0u; k < 3u; k = k + 1u) {
            let key = red_key(c[k]);
            atomicMax(&wg_max, key);
            atomicMax(&wg_min, ~key);
        }
        textureStore(dst, p, c);
    }
    workgroupBarrier();
    if (li == 0u) {
        let slot = u32(arg(0u));
        atomicMax(&R[slot], atomicLoad(&wg_max));
        atomicMax(&R[slot + 1u], atomicLoad(&wg_min));
    }
}
"#;
        let mut p = self.params();
        p.set(0, slot as f64);
        self.pass("reduce_minmax", BODY, &p);
    }

    /// G'MIC `normalize lo,hi` using the min/max in reduction slot `slot` (constant images
    /// become `lo`, like G'MIC).
    #[allow(dead_code)]
    pub fn normalize(&mut self, slot: u32, lo: f64, hi: f64) {
        const BODY: &str = r#"
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let slot = u32(arg(0u));
    let m = red_min(slot);
    let M = red_max(slot);
    let c = textureLoad(src, p, 0);
    var v = vec3<f32>(arg(1u));
    if (M > m) { v = (c.rgb - m) / (M - m) * (arg(2u) - arg(1u)) + arg(1u); }
    textureStore(dst, p, vec4<f32>(v, c.a));
}
"#;
        let mut p = self.params();
        p.set(0, slot as f64).set(1, lo).set(2, hi);
        self.pass("normalize", BODY, &p);
    }
}

/// Runs `effect` on a planar frame and returns the processed planar frame.
pub fn run_frame(
    ctx: &GpuContext,
    targets: &Targets,
    frame: &crate::frame::Frame,
    rgba_scratch: &mut Vec<u8>,
    seed: u32,
    record: impl FnOnce(&mut PassChain) -> Result<(), String>,
) -> Result<crate::frame::Frame, String> {
    let (w, h) = (targets.width, targets.height);
    frame.to_rgba_into(rgba_scratch);
    ctx.queue.write_texture(
        targets.input.as_image_copy(),
        rgba_scratch,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );

    let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
    encoder.clear_buffer(&targets.reduce, 0, None);
    let mut chain = PassChain { ctx, targets, encoder, current: Current::Orig, seed };

    // Load: input (0..1) → orig (0..255). Uses the standard layout with src = input.
    {
        let pipeline = ctx.pipeline("load", LOAD_SHADER);
        let input_view = targets.input.create_view(&Default::default());
        let orig_view = targets.orig.create_view(&Default::default());
        let params = Params::new(w, h);
        let ubuf = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        // `orig` binding needs some rgba32f view other than dst; ping0 is unused at this point.
        let spare = targets.ping[0].create_view(&Default::default());
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("load"),
            layout: &ctx.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&input_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&ctx.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&orig_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&spare) },
                wgpu::BindGroupEntry { binding: 4, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: wgpu::BindingResource::TextureView(&ctx.dummy_aux) },
                wgpu::BindGroupEntry { binding: 6, resource: targets.reduce.as_entire_binding() },
            ],
        });
        let mut cpass = chain.encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("load"), timestamp_writes: None });
        cpass.set_pipeline(&pipeline);
        cpass.set_bind_group(0, &bind, &[]);
        cpass.dispatch_workgroups(w.div_ceil(WG), h.div_ceil(WG), 1);
    }

    record(&mut chain)?;

    // Store: current (0..255) → out8, then copy to the readback buffer.
    let current_view = chain.view_of(chain.current);
    let out_view = targets.out8.create_view(&Default::default());
    let sparams: [f32; 4] = [w as f32, h as f32, 0.0, 0.0];
    let sbuf = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("store params"),
        contents: bytemuck::bytes_of(&sparams),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let sbind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("store"),
        layout: &ctx.store_layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&current_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&out_view) },
            wgpu::BindGroupEntry { binding: 2, resource: sbuf.as_entire_binding() },
        ],
    });
    let mut encoder = chain.encoder;
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("store"), timestamp_writes: None });
        cpass.set_pipeline(&ctx.store_pipeline);
        cpass.set_bind_group(0, &sbind, &[]);
        cpass.dispatch_workgroups(w.div_ceil(WG), h.div_ceil(WG), 1);
    }
    encoder.copy_texture_to_buffer(
        targets.out8.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &targets.readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(targets.padded_row), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    ctx.queue.submit(Some(encoder.finish()));

    // Read back.
    let slice = targets.readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    ctx.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("GPU poll failed: {e}"))?;
    rx.recv()
        .map_err(|_| "GPU readback was dropped".to_string())?
        .map_err(|e| format!("GPU readback failed: {e}"))?;

    let mut out = crate::frame::Frame::new(w, h);
    {
        let data = slice.get_mapped_range().map_err(|e| format!("GPU readback failed: {e:?}"))?;
        let n = (w * h) as usize;
        let (r, rest) = out.data.split_at_mut(n);
        let (g, b) = rest.split_at_mut(n);
        for y in 0..h as usize {
            let row = &data[y * targets.padded_row as usize..y * targets.padded_row as usize + w as usize * 4];
            let base = y * w as usize;
            for (x, px) in row.chunks_exact(4).enumerate() {
                r[base + x] = px[0];
                g[base + x] = px[1];
                b[base + x] = px[2];
            }
        }
    }
    targets.readback.unmap();
    Ok(out)
}
