//! `fx_halftone brightness,contrast,gamma,smoothness,tones,size_dark,size_bright,shape,pattern_smoothness`
//!
//! G'MIC: `adjust_colors` → `blur smoothness` → per channel `halftone tones,dark,bright,shape,smooth`.
//! `halftone` quantizes each channel into N tones (k-means on the histogram), then fills every
//! tone with a tiled dot pattern whose dot size grows from dark to bright tones.
//!
//! GPU version: the dot tiles (one per tone, tiny) and the k-means tone centroids are computed
//! on the CPU per frame and uploaded; a single pass then looks up each pixel's tone tile.
//! The k-means is our own (G'MIC's colormap estimation differs slightly), so tone boundaries
//! can shift a little compared to G'MIC; the dot geometry itself is identical.

use super::{arg, gaussian_blur, GpuEffect};
use crate::frame::Frame;
use crate::gpu::{GpuContext, PassChain};

pub fn supports(args: &[f64]) -> bool {
    let n = arg(args, 4, 5.0).round() as i64;
    (2..=32).contains(&n)
}

#[derive(Default)]
pub struct Halftone {
    /// Tone centroids per channel (sorted).
    centroids: [Vec<f64>; 3],
    sizes: Vec<u32>,
    atlas: Option<(wgpu::Texture, u32, u32)>,
    atlas_key: Option<(i64, i64, i64, i64, i64)>,
}

const ADJUST_BODY: &str = r#"
// arg: 0 brightness, 1 contrast, 2 gamma (G'MIC adjust_colors with value range [0,255])
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    var c = textureLoad(src, p, 0).rgb;
    if (arg(2u) != 0.0) { c = pow(max(c / 255.0, vec3<f32>(0.0)), vec3<f32>(pow(10.0, -arg(2u) / 100.0))) * 255.0; }
    if (arg(1u) != 0.0) { c = (c - 128.0) * exp(arg(1u) / 64.0) + 128.0; }
    c = clamp(c + arg(0u) * 2.0, vec3<f32>(0.0), vec3<f32>(255.0));
    textureStore(dst, p, vec4<f32>(c, 255.0));
}
"#;

const HALFTONE_BODY: &str = r#"
// arg[ch*32 + k] = centroid k of channel ch; arg[96 + k] = tile size of tone k; misc.y = N; misc.w = atlas stride
fn tone_of(v: f32, ch: u32, n: u32) -> u32 {
    var best = 0u;
    var best_d = 1e30;
    for (var k = 0u; k < n; k = k + 1u) {
        let d = abs(v - arg(ch * 32u + k));
        if (d < best_d) { best_d = d; best = k; }
    }
    return best;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let p = vec2<i32>(gid.xy);
    if (p.x >= size_i().x || p.y >= size_i().y) { return; }
    let n = u32(P.misc.y);
    let stride = i32(P.misc.w);
    let c = textureLoad(src, p, 0);
    var out = vec3<f32>(0.0);
    for (var ch = 0u; ch < 3u; ch = ch + 1u) {
        let k = tone_of(c[ch], ch, n);
        let s = i32(arg(96u + k));
        let t = vec2<i32>(p.x % s + i32(k) * stride, p.y % s);
        out[ch] = textureLoad(aux, t, 0).r;
    }
    textureStore(dst, p, vec4<f32>(out, 255.0));
}
"#;

/// G'MIC adjust_colors on one value (brightness, contrast, gamma; range [0,255]).
fn adjust(v: f64, b: f64, c: f64, g: f64) -> f64 {
    let mut v = v;
    if g != 0.0 {
        v = (v / 255.0).max(0.0).powf(10f64.powf(-g / 100.0)) * 255.0;
    }
    if c != 0.0 {
        v = (v - 128.0) * (c / 64.0).exp() + 128.0;
    }
    (v + b * 2.0).clamp(0.0, 255.0)
}

/// G'MIC `colormap N,1,1` on 1-D values: median-cut initialisation, then k-means, sorted.
///
/// `samples` are `(value, multiplicity)`: G'MIC works on the set of *distinct* values, so for
/// integer images every value has multiplicity 1. Multiplicities > 1 approximate float images
/// where nearly every pixel is distinct (e.g. after a pre-blur). When there are no more
/// distinct values than `k`, they are returned as-is (G'MIC skips quantization then).
pub fn gmic_colormap_1d(samples: &[(f64, u64)], k: usize) -> Vec<f64> {
    let mut els: Vec<(f64, u64)> = samples.iter().copied().filter(|s| s.1 > 0 && s.0.is_finite()).collect();
    els.sort_by(|a, b| a.0.total_cmp(&b.0));
    els.dedup_by(|b, a| {
        if a.0 == b.0 {
            a.1 += b.1;
            true
        } else {
            false
        }
    });
    if els.is_empty() {
        return vec![0.0];
    }
    // Prefix sums of multiplicities: element j covers expanded indices [ends[j-1], ends[j]).
    let mut ends = Vec::with_capacity(els.len());
    let mut acc = 0u64;
    for e in &els {
        acc += e.1;
        ends.push(acc);
    }
    let total = acc;
    if total as usize <= k {
        return els.iter().flat_map(|e| std::iter::repeat_n(e.0, e.1 as usize)).collect();
    }
    let elem_at = |i: u64| ends.partition_point(|&e| e <= i);
    let value_at = |i: u64| els[elem_at(i)].0;
    let box_mean = |a: u64, b: u64| {
        let (mut sum, mut cnt) = (0.0, 0u64);
        for j in elem_at(a)..=elem_at(b) {
            let lo = if j == 0 { 0 } else { ends[j - 1] }.max(a);
            let hi = (ends[j] - 1).min(b);
            let c = hi - lo + 1;
            sum += els[j].0 * c as f64;
            cnt += c;
        }
        sum / cnt as f64
    };

    // Median-cut: repeatedly split the first box with the largest value range at its middle index.
    let mut boxes: Vec<(u64, u64)> = vec![(0, total - 1)];
    while boxes.len() < k {
        let (mut max_b, mut max_r) = (0, -1.0);
        for (i, &(a, b)) in boxes.iter().enumerate() {
            let r = value_at(b) - value_at(a);
            if r > max_r {
                max_r = r;
                max_b = i;
            }
        }
        if max_r <= 0.0 {
            break;
        }
        let (a, b) = boxes[max_b];
        let pivot = (a + b) / 2;
        boxes[max_b] = (a, pivot);
        boxes.push((pivot + 1, b));
    }
    let mut cmap: Vec<f64> = boxes.iter().map(|&(a, b)| box_mean(a, b)).collect();

    // K-means; G'MIC stops once (max centroid change) / k <= (range + 1) / 8192.
    let (lo, hi) = cmap.iter().fold((f64::MAX, f64::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let max_diff = (hi - lo + 1.0) / 8192.0;
    for _ in 0..1000 {
        let mut sum = vec![0.0; cmap.len()];
        let mut cnt = vec![0u64; cmap.len()];
        for &(v, m) in &els {
            let j = nearest(&cmap, v);
            sum[j] += v * m as f64;
            cnt[j] += m;
        }
        let mut diff = 0.0f64;
        for j in 0..cmap.len() {
            if cnt[j] > 0 {
                let c = sum[j] / cnt[j] as f64;
                diff = diff.max((c - cmap[j]).abs());
                cmap[j] = c;
            }
        }
        if diff / cmap.len() as f64 <= max_diff {
            break;
        }
    }
    cmap.sort_by(|a, b| a.total_cmp(b));
    cmap
}

/// Approximate histogram of the pre-blurred channel (float values, nearly all distinct, so
/// multiplicity = pixel count). Computed on a subsampled grid to keep it cheap.
fn blurred_histogram(frame: &Frame, ch: usize, lut: &[f64], sigma: f64) -> Vec<(f64, u64)> {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let step = (sigma.floor() as usize).clamp(1, (w / 320).max(1));
    let (sw, sh) = (w.div_ceil(step), h.div_ceil(step));
    let plane = frame.plane(ch);
    let mut img = vec![0.0f64; sw * sh];
    for y in 0..sh {
        for x in 0..sw {
            img[y * sw + x] = lut[plane[(y * step) * w + x * step] as usize];
        }
    }
    let s = sigma / step as f64;
    let r = (3.0 * s).ceil() as i64;
    let kernel: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * s * s)).exp()).collect();
    let ksum: f64 = kernel.iter().sum();
    let mut tmp = vec![0.0; sw * sh];
    for y in 0..sh {
        for x in 0..sw {
            let mut acc = 0.0;
            for (j, kv) in kernel.iter().enumerate() {
                let xx = (x as i64 + j as i64 - r).clamp(0, sw as i64 - 1) as usize;
                acc += kv * img[y * sw + xx];
            }
            tmp[y * sw + x] = acc / ksum;
        }
    }
    const BINS: f64 = 16.0;
    let mut hist = vec![0u64; (255.0 * BINS) as usize + 1];
    for y in 0..sh {
        for x in 0..sw {
            let mut acc = 0.0;
            for (j, kv) in kernel.iter().enumerate() {
                let yy = (y as i64 + j as i64 - r).clamp(0, sh as i64 - 1) as usize;
                acc += kv * tmp[yy * sw + x];
            }
            hist[((acc / ksum).clamp(0.0, 255.0) * BINS).round() as usize] += 1;
        }
    }
    hist[0] = hist[0].max(1);
    let last = hist.len() - 1;
    hist[last] = hist[last].max(1);
    hist.iter().enumerate().filter(|(_, c)| **c > 0).map(|(i, &c)| (i as f64 / BINS, c)).collect()
}

fn nearest(c: &[f64], v: f64) -> usize {
    let mut best = 0;
    for (i, ci) in c.iter().enumerate() {
        if (v - ci).abs() < (v - c[best]).abs() {
            best = i;
        }
    }
    best
}

/// Final value of tone `k`'s tiled pattern (G'MIC `halftone` inner loop, accumulated result).
pub fn tone_tile(n: usize, k: usize, size: usize, shape: i64, smooth: f64) -> Vec<f32> {
    let s = size.max(1);
    let c = ((s as f64 - 1.0) / 2.0).round();
    let metric = shape % 3;
    let dist = |x: f64, y: f64| {
        let (dx, dy) = ((x - c).abs(), (y - c).abs());
        match metric {
            0 => dx.max(dy),
            1 => dx + dy,
            _ => (dx * dx + dy * dy).sqrt(),
        }
    };
    let sh = (s as f64 / 2.0).round() as usize;
    let mut pat = vec![0.0f64; s * s];
    for y in 0..s {
        for x in 0..s {
            let p1 = dist(x as f64, y as f64);
            let (xs, ys) = ((x + s - sh % s) % s, (y + s - sh % s) % s);
            let p2 = dist(xs as f64, ys as f64);
            pat[y * s + x] = p1.min(p2);
        }
    }
    let max_p = pat.iter().cloned().fold(0.0, f64::max);
    let frac = if shape >= 3 { (n - 1 - k) as f64 / (n as f64 - 1.1) } else { k as f64 / (n as f64 - 1.1) };
    let thr = frac * max_p;
    let mut bin: Vec<f64> = pat.iter().map(|&p| if p < thr { 255.0 } else { 0.0 }).collect();
    if smooth >= 0.25 {
        bin = blur_periodic(&bin, s, smooth);
    }
    bin.iter().map(|&b| if shape >= 3 { (255.0 - b) as f32 } else { b as f32 }).collect()
}

fn blur_periodic(img: &[f64], s: usize, sigma: f64) -> Vec<f64> {
    let r = (3.0 * sigma).ceil() as i64;
    let kernel: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let ksum: f64 = kernel.iter().sum();
    let si = s as i64;
    let mut tmp = vec![0.0; s * s];
    for y in 0..s {
        for x in 0..s {
            let mut acc = 0.0;
            for (j, kv) in kernel.iter().enumerate() {
                let xx = ((x as i64 + j as i64 - r) % si + si) % si;
                acc += kv * img[y * s + xx as usize];
            }
            tmp[y * s + x] = acc / ksum;
        }
    }
    let mut out = vec![0.0; s * s];
    for y in 0..s {
        for x in 0..s {
            let mut acc = 0.0;
            for (j, kv) in kernel.iter().enumerate() {
                let yy = ((y as i64 + j as i64 - r) % si + si) % si;
                acc += kv * tmp[yy as usize * s + x];
            }
            out[y * s + x] = acc / ksum;
        }
    }
    out
}

impl GpuEffect for Halftone {
    fn prepare(&mut self, ctx: &GpuContext, frame: &Frame, args: &[f64]) -> Result<(), String> {
        let (b, c, g) = (arg(args, 0, 0.0), arg(args, 1, 0.0), arg(args, 2, 0.0));
        let n = arg(args, 4, 5.0).round().clamp(2.0, 32.0) as usize;
        let dark = arg(args, 5, 8.0).round().max(2.0);
        let bright = arg(args, 6, 8.0).round().max(2.0);
        let shape = arg(args, 7, 5.0).round().clamp(0.0, 5.0) as i64;
        let smooth = arg(args, 8, 0.1).max(0.0);

        // Tone centroids per channel: G'MIC's `quantize N,0` = `colormap N,1,1` on the distinct
        // values of the adjusted (and pre-blurred) channel, plus 0 and 255 from the row it appends.
        let pre_blur = arg(args, 3, 0.0).max(0.0);
        let lut: Vec<f64> = (0..256).map(|v| adjust(v as f64, b, c, g) as f32 as f64).collect();
        for ch in 0..3 {
            let samples: Vec<(f64, u64)> = if pre_blur > 0.0 {
                blurred_histogram(frame, ch, &lut, pre_blur)
            } else {
                let mut seen = [false; 256];
                for &v in frame.plane(ch) {
                    seen[v as usize] = true;
                }
                let mut vals: Vec<f64> = (0..256).filter(|&v| seen[v]).map(|v| lut[v]).collect();
                vals.extend([0.0, 255.0]);
                vals.sort_by(|a, b| a.total_cmp(b));
                vals.dedup();
                vals.into_iter().map(|v| (v, 1)).collect()
            };
            self.centroids[ch] = gmic_colormap_1d(&samples, n);
        }

        // Dot tiles (cached while geometry parameters stay the same).
        let key = (n as i64, dark as i64, bright as i64, shape, (smooth * 1000.0).round() as i64);
        if self.atlas_key != Some(key) {
            self.sizes = (0..n)
                .map(|k| (((dark * (n - 1 - k) as f64) + bright * k as f64) / (n as f64 - 1.0)).round().max(1.0) as u32)
                .collect();
            let stride = *self.sizes.iter().max().unwrap();
            let (aw, ah) = (stride * n as u32, stride);
            let mut data = vec![0f32; (aw * ah * 4) as usize];
            for (k, &s) in self.sizes.iter().enumerate() {
                let tile = tone_tile(n, k, s as usize, shape, smooth);
                for y in 0..s {
                    for x in 0..s {
                        let o = ((y * aw + k as u32 * stride + x) * 4) as usize;
                        data[o] = tile[(y * s + x) as usize];
                    }
                }
            }
            let tex = ctx.texture(
                aw,
                ah,
                wgpu::TextureFormat::Rgba32Float,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                "halftone atlas",
            );
            ctx.queue.write_texture(
                tex.as_image_copy(),
                bytemuck::cast_slice(&data),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(aw * 16), rows_per_image: Some(ah) },
                wgpu::Extent3d { width: aw, height: ah, depth_or_array_layers: 1 },
            );
            self.atlas = Some((tex, aw, stride));
            self.atlas_key = Some(key);
        }
        Ok(())
    }

    fn encode(&mut self, chain: &mut PassChain, args: &[f64]) -> Result<(), String> {
        let (b, c, g) = (arg(args, 0, 0.0), arg(args, 1, 0.0), arg(args, 2, 0.0));
        let pre_blur = arg(args, 3, 0.0).max(0.0);

        if b != 0.0 || c != 0.0 || g != 0.0 {
            let mut p = chain.params();
            p.set(0, b).set(1, c).set(2, g);
            chain.pass("halftone_adjust", ADJUST_BODY, &p);
        }
        if pre_blur > 0.0 {
            gaussian_blur(chain, pre_blur, pre_blur, 1);
        }

        let Some((atlas, _aw, stride)) = &self.atlas else { return Err("halftone atlas missing".into()) };
        let n = self.sizes.len();
        let mut p = chain.params();
        for ch in 0..3 {
            // Fewer distinct values than tones: unused tones get unreachable centroids.
            for k in 0..n {
                p.set(ch * 32 + k, self.centroids[ch].get(k).copied().unwrap_or(1e9));
            }
        }
        for (k, s) in self.sizes.iter().enumerate() {
            p.set(96 + k, *s as f64);
        }
        p.misc[1] = n as f32;
        p.misc[3] = *stride as f32;
        let view = atlas.create_view(&Default::default());
        chain.pass_with_aux("halftone", HALFTONE_BODY, &p, Some(&view));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colormap_separates_clusters() {
        let samples = vec![(10.0, 1), (12.0, 1), (200.0, 1), (205.0, 1)];
        let c = gmic_colormap_1d(&samples, 2);
        assert!((c[0] - 11.0).abs() < 0.5 && (c[1] - 202.5).abs() < 0.5, "{c:?}");
        // Not more distinct values than tones: returned unchanged.
        assert_eq!(gmic_colormap_1d(&[(0.0, 1), (255.0, 1)], 5), vec![0.0, 255.0]);
    }

    #[test]
    fn tiles_match_gmic_extremes() {
        // Darkest tone is all black, brightest all white (normal shapes).
        assert!(tone_tile(5, 0, 8, 2, 0.0).iter().all(|&v| v == 0.0));
        assert!(tone_tile(5, 4, 8, 2, 0.0).iter().all(|&v| v == 255.0));
        // Inverted shapes swap them.
        assert!(tone_tile(5, 0, 8, 5, 0.0).iter().all(|&v| v == 0.0));
        assert!(tone_tile(5, 4, 8, 5, 0.0).iter().all(|&v| v == 255.0));
    }
}
