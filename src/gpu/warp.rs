//! G'MIC deformation filters as GPU remaps ("baked" warp maps).
//!
//! Many deformations only move pixels around. Running such a filter on a *coordinate image*
//! gives, for every output pixel, the source position it was taken from: a warp map. The GPU
//! then applies the map to every frame with one bilinear lookup, so the expensive G'MIC run
//! happens only when the parameters change.
//!
//! Each coordinate is encoded as an angle, `(cos, sin)` in channels 0 and 1, so filters with
//! periodic boundaries (e.g. twirl, which warps in polar space) do not corrupt the seam where
//! the coordinate wraps around. Two runs are needed (x, then y). Channel 2 = 255 is a validity
//! mask: filters that fill outside areas with black (Dirichlet boundaries) give 0 there, and
//! at the edges it tells how much of the pixel was inside.
//!
//! Which filters can be baked is decided offline by `classify` (`--classify-warps`), which
//! compares the real G'MIC output with the baked remap on two test images and writes
//! `assets/gpu/warp_bakeable.json`. Random filters and filters that change colours fail the
//! comparison and keep using G'MIC. Maps are only valid for fixed parameters, so animated
//! parameters fall back to the CPU (see `GpuEffectInfo::static_only`).

use super::effects::{GpuEffect, GpuEffectInfo};
use super::{GpuContext, PassChain};
use crate::frame::Frame;
use crate::gmic::{build_gmic_command, FilterDatabase, GmicParamType};
use crate::gmic_worker::GmicWorker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

/// Minimum PSNR (dB) between G'MIC output and the baked remap (both slightly blurred, since
/// G'MIC chaining several interpolated warps is softer than one lookup) to count as a warp.
pub const BAKE_PSNR_THRESHOLD: f64 = 32.0;
/// Minimum PSNR without blur (catches wrong fine detail).
pub const BAKE_PSNR_EXACT_MIN: f64 = 24.0;

/// A baked map: for each output pixel the source position (pixels) and coverage (0..1).
pub struct WarpMap {
    pub width: u32,
    pub height: u32,
    /// Interleaved `[x, y, coverage, 0]` per pixel (RGBA32F texture layout).
    pub data: Vec<f32>,
}

/// Planar coordinate image for axis 0 (x) or 1 (y): the coordinate as an angle,
/// `127.5 + 127.5 * (cos, sin)`, plus a constant 255 mask (all values within 0..255, so
/// filters that clamp to 0..255 still work).
pub fn coord_image(w: u32, h: u32, axis: usize) -> Vec<f32> {
    let n = (w * h) as usize;
    let mut d = vec![255.0f32; n * 3];
    let len = if axis == 0 { w } else { h } as f64;
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let c = if axis == 0 { x } else { y } as f64;
            let a = std::f64::consts::TAU * c / len;
            d[i] = (127.5 + 127.5 * a.cos()) as f32;
            d[n + i] = (127.5 + 127.5 * a.sin()) as f32;
        }
    }
    d
}

/// Runs `command_line` on coordinate images of `w`×`h` and converts the result to a map.
pub fn bake(worker: &mut GmicWorker, w: u32, h: u32, command_line: &str, seed: u32) -> Result<WarpMap, String> {
    let n = (w * h) as usize;
    let mut data = vec![0f32; n * 4];
    for axis in 0..2 {
        let (ow, oh, out) = worker.process_f32(w, h, &coord_image(w, h, axis), command_line, seed)?;
        if (ow, oh) != (w, h) {
            return Err(format!("warp map has size {ow}x{oh}, expected {w}x{h}"));
        }
        let len = if axis == 0 { w } else { h } as f64;
        for i in 0..n {
            let m = out[2 * n + i] as f64 / 255.0;
            if m <= 1e-3 {
                data[i * 4 + 2] = 0.0;
                continue;
            }
            let (c, s) = (out[i] as f64 / m - 127.5, out[n + i] as f64 / m - 127.5);
            let a = s.atan2(c).rem_euclid(std::f64::consts::TAU);
            // Angle → coordinate; values just below 0 wrap to ~len, clamp them back.
            let mut v = a / std::f64::consts::TAU * len;
            if v > len - 0.5 {
                v -= len;
            }
            data[i * 4 + axis] = v.max(0.0) as f32;
            if axis == 0 {
                data[i * 4 + 2] = m.min(1.0) as f32;
            } else {
                data[i * 4 + 2] = data[i * 4 + 2].min(m.min(1.0) as f32);
            }
        }
    }
    Ok(WarpMap { width: w, height: h, data })
}

/// Largest map step between neighbouring pixels that still counts as continuous (bigger jumps
/// are seams, e.g. kaleidoscope mirrors, and must not be blurred across).
const MAX_STEP: f32 = 8.0;
/// Maximum supersampling per axis when the warp squeezes many source pixels into one.
const MAX_SAMPLES: f32 = 4.0;

/// Map step from pixel `i` to its neighbour `i ± stride` (whichever side is continuous).
fn map_step(map: &WarpMap, i: usize, fwd: Option<usize>, back: Option<usize>) -> [f32; 2] {
    let at = |j: usize| [map.data[j * 4], map.data[j * 4 + 1]];
    let ok = |j: usize| map.data[j * 4 + 2] > 1e-3;
    let p = at(i);
    let step = |a: [f32; 2], b: [f32; 2]| [b[0] - a[0], b[1] - a[1]];
    let small = |d: [f32; 2]| d[0].abs().max(d[1].abs()) <= MAX_STEP;
    if let Some(j) = fwd.filter(|&j| ok(j)) {
        let d = step(p, at(j));
        if small(d) {
            return d;
        }
    }
    if let Some(j) = back.filter(|&j| ok(j)) {
        let d = step(at(j), p);
        if small(d) {
            return d;
        }
    }
    [0.0, 0.0]
}

/// CPU version of the GPU remap (used by the classifier and tests): bilinear lookups,
/// optionally supersampled over the pixel's footprint where the warp shrinks the image
/// (matches filters that warp in several interpolated steps, like twirl).
pub fn remap_cpu(frame: &Frame, map: &WarpMap, supersample: bool) -> Frame {
    let (w, h) = (frame.width as i64, frame.height as i64);
    let (mw, mh) = (map.width as usize, map.height as usize);
    let n = frame.plane_len();
    let mut out = Frame::new(map.width, map.height);
    let on = out.plane_len();
    let sample = |x: f32, y: f32, acc: &mut [f32; 3]| {
        let (x0, y0) = (x.floor(), y.floor());
        let (dx, dy) = (x - x0, y - y0);
        let at = |xx: i64, yy: i64| (yy.clamp(0, h - 1) * w + xx.clamp(0, w - 1)) as usize;
        let (xi, yi) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (at(xi, yi), at(xi + 1, yi), at(xi, yi + 1), at(xi + 1, yi + 1));
        for (ch, v) in acc.iter_mut().enumerate() {
            let p = &frame.data[ch * n..(ch + 1) * n];
            let top = p[a] as f32 * (1.0 - dx) + p[b] as f32 * dx;
            let bot = p[c] as f32 * (1.0 - dx) + p[d] as f32 * dx;
            *v += top * (1.0 - dy) + bot * dy;
        }
    };
    for i in 0..on {
        let m = map.data[i * 4 + 2];
        if m <= 1e-3 {
            continue;
        }
        let (px, py) = (i % mw, i / mw);
        let (jx, jy, nx, ny) = if supersample {
            let jx = map_step(map, i, (px + 1 < mw).then_some(i + 1), (px > 0).then(|| i - 1));
            let jy = map_step(map, i, (py + 1 < mh).then_some(i + mw), (py > 0).then(|| i - mw));
            let nx = (jx[0].abs().max(jx[1].abs()).ceil()).clamp(1.0, MAX_SAMPLES);
            let ny = (jy[0].abs().max(jy[1].abs()).ceil()).clamp(1.0, MAX_SAMPLES);
            (jx, jy, nx, ny)
        } else {
            ([0.0; 2], [0.0; 2], 1.0, 1.0)
        };
        let (cx, cy) = (map.data[i * 4], map.data[i * 4 + 1]);
        let mut acc = [0f32; 3];
        for sy in 0..ny as usize {
            let fy = (sy as f32 + 0.5) / ny - 0.5;
            for sx in 0..nx as usize {
                let fx = (sx as f32 + 0.5) / nx - 0.5;
                sample(cx + fx * jx[0] + fy * jy[0], cy + fx * jx[1] + fy * jy[1], &mut acc);
            }
        }
        let k = m / (nx * ny);
        for (ch, v) in acc.iter().enumerate() {
            out.data[ch * on + i] = (v * k).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

const REMAP_BODY: &str = r#"
// aux = warp map (rgba32f): source x, source y, coverage. Same size as the frame.
// arg(0) = supersample (0/1). Must match `remap_cpu`.
const MAX_STEP: f32 = 8.0;
const MAX_SAMPLES: f32 = 4.0;

fn map_ok(q: vec2<i32>) -> bool {
    let s = vec2<i32>(textureDimensions(aux));
    return q.x >= 0 && q.y >= 0 && q.x < s.x && q.y < s.y && textureLoad(aux, q, 0).z > 0.001;
}

fn map_step(p: vec2<i32>, e: vec2<i32>, c: vec2<f32>) -> vec2<f32> {
    if (map_ok(p + e)) {
        let d = textureLoad(aux, p + e, 0).xy - c;
        if (max(abs(d.x), abs(d.y)) <= MAX_STEP) { return d; }
    }
    if (map_ok(p - e)) {
        let d = c - textureLoad(aux, p - e, 0).xy;
        if (max(abs(d.x), abs(d.y)) <= MAX_STEP) { return d; }
    }
    return vec2<f32>(0.0);
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let m = textureLoad(aux, p, 0);
    if (m.z <= 0.001) { textureStore(dst, p, vec4<f32>(0.0, 0.0, 0.0, 255.0)); return; }
    var jx = vec2<f32>(0.0);
    var jy = vec2<f32>(0.0);
    if (arg(0u) > 0.5) {
        jx = map_step(p, vec2<i32>(1, 0), m.xy);
        jy = map_step(p, vec2<i32>(0, 1), m.xy);
    }
    let nx = clamp(ceil(max(abs(jx.x), abs(jx.y))), 1.0, MAX_SAMPLES);
    let ny = clamp(ceil(max(abs(jy.x), abs(jy.y))), 1.0, MAX_SAMPLES);
    var acc = vec3<f32>(0.0);
    for (var sy = 0.0; sy < ny; sy = sy + 1.0) {
        let fy = (sy + 0.5) / ny - 0.5;
        for (var sx = 0.0; sx < nx; sx = sx + 1.0) {
            let fx = (sx + 0.5) / nx - 0.5;
            let q = m.xy + fx * jx + fy * jy;
            acc = acc + bilinear(src, q.x, q.y, 1).rgb;
        }
    }
    textureStore(dst, p, vec4<f32>(acc * (m.z / (nx * ny)), 255.0));
}
"#;

/// GPU effect for one bakeable filter. The command line comes from `set_command_line`.
#[derive(Default)]
pub struct WarpEffect {
    command_line: String,
    seed: u32,
    worker: Option<GmicWorker>,
    /// (command line, width, height) of the uploaded map.
    key: Option<(String, u32, u32)>,
    texture: Option<wgpu::Texture>,
}

impl GpuEffect for WarpEffect {
    fn set_command_line(&mut self, command_line: &str, seed: u32) {
        self.command_line = command_line.to_string();
        self.seed = seed;
    }

    fn prepare(&mut self, ctx: &GpuContext, frame: &Frame, _args: &[f64]) -> Result<(), String> {
        let (w, h) = (frame.width, frame.height);
        let key = (self.command_line.clone(), w, h);
        if self.key.as_ref() == Some(&key) && self.texture.is_some() {
            return Ok(());
        }
        if self.command_line.is_empty() {
            return Err("warp effect has no command line".into());
        }
        if self.worker.as_ref().is_none_or(|w| w.is_dead()) {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            self.worker = Some(GmicWorker::spawn(threads)?);
        }
        let map = bake(self.worker.as_mut().unwrap(), w, h, &self.command_line, self.seed)?;
        let tex = ctx.texture(
            w,
            h,
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            "warp map",
        );
        ctx.queue.write_texture(
            tex.as_image_copy(),
            bytemuck::cast_slice(&map.data),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 16), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.texture = Some(tex);
        self.key = Some(key);
        Ok(())
    }

    fn encode(&mut self, chain: &mut PassChain, _args: &[f64]) -> Result<(), String> {
        let tex = self.texture.as_ref().ok_or("warp map missing")?;
        let view = tex.create_view(&Default::default());
        let command = self.command_line.split_whitespace().next().unwrap_or("");
        let mut p = chain.params();
        p.set(0, if supersample_for(command) { 1.0 } else { 0.0 });
        chain.pass_with_aux("warp_remap", REMAP_BODY, &p, Some(&view));
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------
// Classification and registry
// ------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarpEntry {
    pub command: String,
    pub name: String,
    /// Lowest PSNR (dB) of the baked remap vs. G'MIC on the test images.
    pub psnr: f64,
    /// Remap with footprint supersampling (closer to G'MIC for multi-step warps).
    #[serde(default)]
    pub supersample: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rejected {
    pub command: String,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WarpClassification {
    pub threshold_db: f64,
    pub bakeable: Vec<WarpEntry>,
    pub rejected: Vec<Rejected>,
}

fn file_candidates() -> Vec<PathBuf> {
    let rel = "assets/gpu/warp_bakeable.json";
    let mut c = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            c.push(dir.join(rel));
            c.push(dir.join("../share/gmicvideo").join(rel));
            c.push(dir.join("../..").join(rel));
        }
    }
    c.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel));
    if let Ok(cwd) = std::env::current_dir() {
        c.push(cwd.join(rel));
    }
    c
}

/// Path the classifier writes to (the project's assets directory when available).
pub fn default_output_path() -> PathBuf {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/gpu");
    if project.is_dir() || project.parent().is_some_and(|p| p.is_dir()) {
        project.join("warp_bakeable.json")
    } else {
        PathBuf::from("warp_bakeable.json")
    }
}

fn classification() -> &'static WarpClassification {
    static CLS: OnceLock<WarpClassification> = OnceLock::new();
    CLS.get_or_init(|| {
        let Some(text) = file_candidates().into_iter().find_map(|p| std::fs::read_to_string(p).ok()) else {
            return WarpClassification::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|_| {
            eprintln!("[GPU] warp_bakeable.json is invalid; deformations use the CPU");
            WarpClassification::default()
        })
    })
}

fn supersample_for(command: &str) -> bool {
    classification().bakeable.iter().any(|e| e.command == command && e.supersample)
}

/// Registry entries for every bakeable filter (loaded once from the JSON file).
pub fn registry() -> &'static [GpuEffectInfo] {
    static REG: OnceLock<&'static [GpuEffectInfo]> = OnceLock::new();
    REG.get_or_init(|| {
        let infos: Vec<GpuEffectInfo> = classification()
            .bakeable
            .iter()
            .cloned()
            .map(|e| {
                let command: &'static str = Box::leak(e.command.into_boxed_str());
                GpuEffectInfo {
                    name: Box::leak(e.name.into_boxed_str()),
                    commands: Box::leak(vec![command].into_boxed_slice()),
                    supports: |_| true,
                    create: || Box::new(WarpEffect::default()),
                    static_only: true,
                }
            })
            .collect();
        Box::leak(infos.into_boxed_slice())
    })
}

/// A second test image: smooth colour waves plus fine texture (so remaps of any kind show up).
fn synthetic_image(w: u32, h: u32) -> Frame {
    let mut f = Frame::new(w, h);
    let n = f.plane_len();
    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f64 / w as f64, y as f64 / h as f64);
            let i = (y * w + x) as usize;
            let r = 128.0 + 100.0 * (xf * 9.0 + yf * 3.0).sin() + 25.0 * ((x * 7 + y * 3) % 11) as f64 / 11.0;
            let g = 128.0 + 100.0 * (yf * 11.0 - xf * 4.0).cos();
            let b = 128.0 + 90.0 * ((xf - 0.5).hypot(yf - 0.5) * 25.0).sin();
            f.data[i] = r.clamp(0.0, 255.0) as u8;
            f.data[n + i] = g.clamp(0.0, 255.0) as u8;
            f.data[2 * n + i] = b.clamp(0.0, 255.0) as u8;
        }
    }
    f
}

fn test_image(w: u32) -> Option<Frame> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Tests/test_image.jpeg");
    let info = crate::video::probe_video(&path).ok()?;
    let h = ((info.height as f64 * w as f64 / info.width as f64) / 2.0).round() as u32 * 2;
    crate::video::extract_frame(&info, 0.0, w, h).ok()
}

/// 3×3 binomial blur (clamped edges).
fn blur3(f: &Frame) -> Frame {
    let (w, h) = (f.width as i64, f.height as i64);
    let n = f.plane_len();
    let mut out = f.clone();
    for c in 0..3 {
        let p = &f.data[c * n..(c + 1) * n];
        for y in 0..h {
            for x in 0..w {
                let mut acc = 0u32;
                for (dy, wy) in [(-1, 1), (0, 2), (1, 1)] {
                    for (dx, wx) in [(-1, 1), (0, 2), (1, 1)] {
                        let (xx, yy) = ((x + dx).clamp(0, w - 1), (y + dy).clamp(0, h - 1));
                        acc += wx * wy * p[(yy * w + xx) as usize] as u32;
                    }
                }
                out.data[c * n + (y * w + x) as usize] = ((acc + 8) / 16) as u8;
            }
        }
    }
    out
}

/// Runs `f` with `worker`, killing it if it takes longer than `timeout`.
fn with_timeout<T>(worker: &mut GmicWorker, timeout: Duration, f: impl FnOnce(&mut GmicWorker) -> T) -> T {
    let kill = worker.kill_handle();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let watchdog = std::thread::spawn(move || {
        if rx.recv_timeout(timeout).is_err() {
            kill.kill();
        }
    });
    let r = f(worker);
    let _ = tx.send(());
    let _ = watchdog.join();
    r
}

/// Decides whether one filter (with default parameters) is a pure warp. Returns the worst
/// (soft) PSNR and whether supersampled remapping matches better.
pub fn classify_filter(worker: &mut GmicWorker, command_line: &str, images: &[Frame]) -> Result<(f64, bool), String> {
    let mut cases = Vec::new();
    for (k, img) in images.iter().enumerate() {
        let real = worker.process(img, command_line, 1)?;
        if k == 0 {
            let other_seed = worker.process(img, command_line, 2)?;
            if other_seed.psnr(&real) < 60.0 {
                return Err("random (output depends on the seed)".into());
            }
        }
        let map = bake(worker, img.width, img.height, command_line, 1)?;
        cases.push((img, real, map));
    }
    // (worst soft PSNR, worst exact PSNR) for one remap mode.
    let score = |supersample: bool| {
        cases.iter().fold((f64::INFINITY, f64::INFINITY), |(s, e), (img, real, map)| {
            let baked = remap_cpu(img, map, supersample);
            (s.min(blur3(&baked).psnr(&blur3(real))), e.min(baked.psnr(real)))
        })
    };
    let (plain, smooth) = (score(false), score(true));
    let (supersample, (soft, exact)) = if smooth.0 > plain.0 + 0.5 { (true, smooth) } else { (false, plain) };
    if soft < BAKE_PSNR_THRESHOLD || exact < BAKE_PSNR_EXACT_MIN {
        return Err(format!("not a pure warp ({soft:.1} dB, exact {exact:.1} dB)"));
    }
    Ok((soft.min(99.0), supersample))
}

/// Classifies every filter of the "Deformations" category (default parameters).
pub fn classify(db: &FilterDatabase, mut progress: impl FnMut(&str, &Result<(f64, bool), String>)) -> WarpClassification {
    let images: Vec<Frame> = test_image(256).into_iter().chain([synthetic_image(256, 144)]).collect();
    let mut result = WarpClassification { threshold_db: BAKE_PSNR_THRESHOLD, ..Default::default() };
    let mut worker: Option<GmicWorker> = None;
    for filter in db.filters.iter().filter(|f| f.category.contains("Deformations")) {
        let has_text = filter.parameters.iter().any(|p| matches!(p.param_type, GmicParamType::Text { .. }));
        let cmd = build_gmic_command(filter, &HashMap::new(), &HashMap::new());
        let outcome = if super::effects::registry().iter().any(|e| e.commands.contains(&filter.command.as_str())) {
            Err("has a hand-written GPU version".to_string())
        } else if filter.command.starts_with("x_") || filter.command.contains("interactive") {
            Err("interactive".to_string())
        } else {
            match worker.as_mut().filter(|w| !w.is_dead()) {
                Some(w) => Ok(w),
                None => GmicWorker::spawn(4).map(|w| worker.insert(w)),
            }
            .and_then(|w| with_timeout(w, Duration::from_secs(60), |w| classify_filter(w, &cmd, &images)))
        };
        progress(&filter.name, &outcome);
        match outcome {
            // Text parameters change the formula itself; defaults being a warp says little.
            Ok((psnr, supersample)) if !has_text => result.bakeable.push(WarpEntry {
                command: filter.command.clone(),
                name: filter.name.clone(),
                psnr: (psnr * 10.0).round() / 10.0,
                supersample,
            }),
            Ok(_) => result.rejected.push(Rejected {
                command: filter.command.clone(),
                name: filter.name.clone(),
                reason: "has text parameters".into(),
            }),
            Err(reason) => result.rejected.push(Rejected {
                command: filter.command.clone(),
                name: filter.name.clone(),
                reason,
            }),
        }
    }
    result.bakeable.sort_by(|a, b| a.command.cmp(&b.command));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baked_mirror_and_shift_are_exact() {
        let mut w = GmicWorker::spawn(2).unwrap();
        let img = synthetic_image(64, 40);
        for cmd in ["mirror x", "shift 5,3,0,0,0", "rotate 30,1,0"] {
            let p = classify_filter(&mut w, cmd, std::slice::from_ref(&img)).map(|r| r.0).unwrap_or(0.0);
            assert!(p >= 35.0, "{cmd}: {p:.1} dB");
        }
        // Black outside (Dirichlet): coverage 0 there.
        let map = bake(&mut w, 64, 40, "shift 10,0,0,0,0", 0).unwrap();
        assert_eq!(map.data[2], 0.0);
        assert!((map.data[(20 * 4) + 2] - 1.0).abs() < 1e-5);
        assert!((map.data[20 * 4] - 10.0).abs() < 1e-3, "x of pixel 20 comes from 10: {}", map.data[20 * 4]);
        // Periodic shift: the wrapped seam must not be corrupted.
        let map = bake(&mut w, 64, 40, "shift 10.5,0,0,0,2", 0).unwrap();
        for x in [0usize, 9, 10, 11, 63] {
            let want = (x as f32 - 10.5).rem_euclid(64.0);
            let got = map.data[x * 4];
            let d = (got - want).abs().min(64.0 - (got - want).abs());
            assert!(d < 0.6, "x={x}: got {got}, want {want}");
        }
    }

    #[test]
    fn colour_and_random_filters_are_rejected() {
        let mut w = GmicWorker::spawn(2).unwrap();
        let img = synthetic_image(64, 40);
        assert!(classify_filter(&mut w, "negate", std::slice::from_ref(&img)).is_err());
        assert!(classify_filter(&mut w, "blur 3", std::slice::from_ref(&img)).is_err());
        let r = classify_filter(&mut w, "noise 20", std::slice::from_ref(&img)).unwrap_err();
        assert!(r.contains("random"), "{r}");
    }
}

#[cfg(test)]
mod debug {
    use super::*;

    /// `GV_WARP_CMD="fx_twirl 1,50,50,3" cargo test dump_warp -- --ignored` writes /tmp/warp_{real,baked,diff}.ppm
    #[test]
    #[ignore]
    fn dump_warp() {
        let cmd = std::env::var("GV_WARP_CMD").unwrap_or_else(|_| "fx_twirl 1,50,50,3".into());
        let mut w = GmicWorker::spawn(4).unwrap();
        let img = test_image(256).unwrap();
        let real = w.process(&img, &cmd, 1).unwrap();
        let map = bake(&mut w, img.width, img.height, &cmd, 1).unwrap();
        let baked = remap_cpu(&img, &map, supersample_for(cmd.split_whitespace().next().unwrap()));
        let mut diff = real.clone();
        for (d, (a, b)) in diff.data.iter_mut().zip(real.data.iter().zip(&baked.data)) {
            *d = ((*a as i32 - *b as i32).abs() * 4).min(255) as u8;
        }
        real.write_ppm(std::path::Path::new("/tmp/warp_real.ppm")).unwrap();
        baked.write_ppm(std::path::Path::new("/tmp/warp_baked.ppm")).unwrap();
        diff.write_ppm(std::path::Path::new("/tmp/warp_diff.ppm")).unwrap();
        eprintln!("{cmd}: {:.1} dB", baked.psnr(&real));
    }
}
