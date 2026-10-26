//! GPU effects vs. G'MIC reference output.

use super::effects::find;
use super::processor::GpuProcessor;
use crate::frame::Frame;
use crate::gmic::{build_gmic_command, FilterDatabase};
use crate::gmic_worker::GmicWorker;
use std::collections::HashMap;

pub fn test_frame(width: u32) -> Option<Frame> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Tests/test_image.jpeg");
    let info = crate::video::probe_video(&path).ok()?;
    let h = ((info.height as f64 * width as f64 / info.width as f64) / 2.0).round() as u32 * 2;
    crate::video::extract_frame(&info, 0.0, width, h).ok()
}

/// Gaussian blur on the CPU (for tone-level comparisons of dithered effects).
pub fn blur(f: &Frame, sigma: f64) -> Frame {
    let r = (3.0 * sigma).ceil() as i64;
    let k: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f64 = k.iter().sum();
    let (w, h) = (f.width as i64, f.height as i64);
    let mut out = f.clone();
    for c in 0..3 {
        let plane = f.plane(c as usize).to_vec();
        let mut tmp = vec![0.0f64; plane.len()];
        for y in 0..h {
            for x in 0..w {
                let mut a = 0.0;
                for (j, kv) in k.iter().enumerate() {
                    let xx = (x + j as i64 - r).clamp(0, w - 1);
                    a += kv * plane[(y * w + xx) as usize] as f64;
                }
                tmp[(y * w + x) as usize] = a / ks;
            }
        }
        let n = (w * h) as usize;
        for y in 0..h {
            for x in 0..w {
                let mut a = 0.0;
                for (j, kv) in k.iter().enumerate() {
                    let yy = (y + j as i64 - r).clamp(0, h - 1);
                    a += kv * tmp[(yy * w + x) as usize];
                }
                out.data[c * n + (y * w + x) as usize] = (a / ks).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Runs a filter (by command) with `overrides` (argument index → value) on GPU and G'MIC.
pub fn compare(command: &str, overrides: &[(usize, f64)], frame: &Frame) -> Option<(Frame, Frame)> {
    let db = FilterDatabase::load_default();
    let filter = db.filters.iter().find(|f| f.command == command)?.clone();
    // Map argument index → parameter id.
    let arg_params: Vec<usize> = filter
        .parameters
        .iter()
        .filter(|p| !matches!(p.param_type, crate::gmic::GmicParamType::Separator | crate::gmic::GmicParamType::Note { .. }))
        .map(|p| p.id)
        .collect();
    let mut values = HashMap::new();
    for &(i, v) in overrides {
        values.insert(arg_params[i], v);
    }
    let args = crate::gmic::argument_values(&filter, &values);
    let info = find(command)?;
    assert!((info.supports)(&args), "{command} should support {args:?}");
    let mut gpu = GpuProcessor::from_info(info)?;
    let g = gpu.process_args(frame, &args, 1).expect("gpu");
    let mut worker = GmicWorker::spawn(8).expect("worker");
    let cmd = build_gmic_command(&filter, &values, &HashMap::new());
    let c = worker.process(frame, &cmd, 1).expect("gmic");
    Some((g, c))
}

macro_rules! need_gpu {
    () => {
        if crate::gpu::GpuContext::get().is_none() {
            eprintln!("no GPU; skipping");
            return;
        }
    };
}

#[test]
fn vignette_matches_gmic() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    for ov in [vec![], vec![(0, 200.0), (1, 30.0), (2, 80.0), (3, 255.0), (4, 40.0), (5, 0.0)]] {
        let (g, c) = compare("fx_vignette", &ov, &f).unwrap();
        let p = g.psnr(&c);
        eprintln!("vignette {ov:?}: {p:.1} dB");
        assert!(p > 35.0, "vignette PSNR {p:.1} dB");
    }
}

#[test]
fn gaussian_blur_matches_gmic() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    for ov in [vec![], vec![(0, 8.0), (3, 1.0)], vec![(0, 2.0), (1, 6.0), (4, 0.0)], vec![(0, 5.0), (3, 2.0), (4, 2.0)]] {
        let (g, c) = compare("fx_blur_gaussian", &ov, &f).unwrap();
        let p = g.psnr(&c);
        eprintln!("blur {ov:?}: {p:.1} dB");
        assert!(p > 35.0, "blur PSNR {p:.1} dB");
    }
}

#[test]
fn halftone_tones_match_gmic() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    for ov in [vec![], vec![(4, 3.0), (5, 12.0), (6, 6.0), (7, 2.0)], vec![(0, 20.0), (1, 30.0), (7, 0.0), (8, 1.0)], vec![(3, 2.0), (4, 4.0)]] {
        let (g, c) = compare("fx_halftone", &ov, &f).unwrap();
        if std::env::var("GV_DUMP").is_ok() {
            let _ = g.write_ppm(std::path::Path::new(&format!("/tmp/ht_gpu_{}.ppm", ov.len())));
            let _ = c.write_ppm(std::path::Path::new(&format!("/tmp/ht_gmic_{}.ppm", ov.len())));
        }
        let exact = g.psnr(&c);
        let toned = blur(&g, 4.0).psnr(&blur(&c, 4.0));
        eprintln!("halftone {ov:?}: exact {exact:.1} dB, tones {toned:.1} dB");
        assert!(toned > 24.0, "halftone tone PSNR {toned:.1} dB");
    }
}

#[test]
fn gpu_is_fast_at_1080p() {
    need_gpu!();
    let Some(f) = test_frame(1920) else { return };
    let info = find("fx_halftone").unwrap();
    let mut gpu = GpuProcessor::from_info(info).unwrap();
    let db = FilterDatabase::load_default();
    let filter = db.filters.iter().find(|f| f.command == "fx_halftone").unwrap();
    let args = crate::gmic::argument_values(filter, &HashMap::new());
    gpu.process_args(&f, &args, 0).unwrap(); // warm-up (shader compile)
    let t = std::time::Instant::now();
    for i in 0..10 {
        gpu.process_args(&f, &args, i).unwrap();
    }
    let ms = t.elapsed().as_secs_f64() * 100.0;
    eprintln!("halftone GPU {}x{}: {ms:.1} ms/frame", f.width, f.height);
}

#[test]
fn baked_warps_match_gmic() {
    need_gpu!();
    use crate::effect::{EffectSettings, PreparedEffect, Timeline};
    use crate::playback::FrameProcessor;
    let f = test_frame(480).unwrap();
    let db = FilterDatabase::load_default();
    let timeline = Timeline::new(1.0, 25.0, None, None);
    let mut worker = GmicWorker::spawn(8).expect("worker");
    for command in ["fx_twirl", "fisheye", "fx_symmetrize", "fx_spherize", "fx_symmetrizoscope", "wave"] {
        let filter = db.filters.iter().find(|x| x.command == command).unwrap();
        let effect = PreparedEffect::new(&db, EffectSettings { filter_id: filter.id.clone(), ..Default::default() }).unwrap();
        let Some(mut gpu) = GpuProcessor::for_effect(&effect) else {
            panic!("{command} should be a baked GPU warp");
        };
        let t = std::time::Instant::now();
        let g = gpu.process(&f, &effect, &timeline, 0).expect("gpu");
        let first = t.elapsed();
        let t = std::time::Instant::now();
        let g2 = gpu.process(&f, &effect, &timeline, 1).expect("gpu");
        let cached = t.elapsed();
        assert_eq!(g, g2, "static warp: same map for every frame");
        let c = worker.process(&f, &effect.command_for_frame(&timeline, 0), 0).expect("gmic");
        let p = blur(&g, 1.0).psnr(&blur(&c, 1.0));
        eprintln!("{command}: {p:.1} dB (exact {:.1}), bake {first:?}, cached {cached:?}", g.psnr(&c));
        assert!(p > 28.0, "{command} PSNR {p:.1} dB");
    }
}

#[test]
fn animated_warp_uses_cpu() {
    need_gpu!();
    use crate::effect::{EffectSettings, PreparedEffect};
    use crate::tween::{AnimationMode, ParameterTween};
    let db = FilterDatabase::load_default();
    let filter = db.filters.iter().find(|x| x.command == "fx_twirl").unwrap();
    let pid = filter.parameters.iter().find(|p| matches!(p.param_type, crate::gmic::GmicParamType::Float { .. })).unwrap().id;
    let mut tweens = HashMap::new();
    tweens.insert(pid, ParameterTween { mode: AnimationMode::Curve, enabled: true, ..Default::default() });
    let animated = PreparedEffect::new(&db, EffectSettings { filter_id: filter.id.clone(), tweens, ..Default::default() }).unwrap();
    assert!(!GpuProcessor::would_handle(&animated));
    let fixed = PreparedEffect::new(&db, EffectSettings { filter_id: filter.id.clone(), ..Default::default() }).unwrap();
    assert!(GpuProcessor::would_handle(&fixed));
}

#[test]
fn unsharp_matches_gmic() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    // Gaussian type is exact; bilateral differs slightly (G'MIC uses a bilateral grid).
    for (ov, min_db) in [
        (vec![(0, 0.0)], 38.0),
        (vec![(0, 0.0), (1, 3.0), (3, 4.0), (5, 0.5), (6, 2.0), (7, 2.0)], 28.0), // differences amplified ~16x
        (vec![(0, 0.0), (4, 5.0), (8, 1.0)], 33.0),
        (vec![], 30.0),
    ] {
        let (g, c) = compare("fx_unsharp", &ov, &f).unwrap();
        let p = g.psnr(&c);
        eprintln!("unsharp {ov:?}: {p:.1} dB");
        assert!(p > min_db, "unsharp {ov:?}: {p:.1} dB");
    }
}

#[test]
fn vhs_matches_gmic_without_noise() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    for ov in [vec![(1, 0.0)], vec![(0, 10.0), (1, 0.0), (2, 40.0), (3, 2.0), (4, 0.5)]] {
        let (g, c) = compare("fx_vhs_filter", &ov, &f).unwrap();
        let p = g.psnr(&c);
        eprintln!("vhs {ov:?}: {p:.1} dB");
        assert!(p > 34.0, "vhs {ov:?}: {p:.1} dB");
    }
}

/// Standard deviation of (out - in) over all values.
fn residual_std(out: &Frame, inp: &Frame) -> f64 {
    let n = out.data.len() as f64;
    let d: Vec<f64> = out.data.iter().zip(&inp.data).map(|(&a, &b)| a as f64 - b as f64).collect();
    let mean = d.iter().sum::<f64>() / n;
    (d.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt()
}

#[test]
fn noise_has_gmic_statistics() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    for ov in [vec![], vec![(0, 30.0), (1, 1.0)], vec![(0, 5.0), (1, 2.0)], vec![(1, 3.0), (2, 0.0)]] {
        let (g, c) = compare("fx_noise", &ov, &f).unwrap();
        let (sg, sc) = (residual_std(&g, &f), residual_std(&c, &f));
        eprintln!("noise {ov:?}: gpu σ {sg:.2}, gmic σ {sc:.2}");
        assert!((sg - sc).abs() <= sc * 0.15 + 0.5, "noise {ov:?}: gpu σ {sg:.2} vs gmic σ {sc:.2}");
        // Same image underneath: heavy blur removes the noise.
        let p = blur(&g, 3.0).psnr(&blur(&c, 3.0));
        assert!(p > 28.0, "noise {ov:?}: blurred PSNR {p:.1}");
    }
}

#[test]
fn ultrawarp2_matches_gmic_without_randomness() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    // All "Range" arguments 0 and no "Random" choices -> deterministic in G'MIC too.
    let fixed: Vec<(usize, f64)> = [4, 6, 8, 10, 21, 23, 25, 27, 29, 31, 33, 35].iter().map(|&i| (i, 0.0)).collect();
    let cases: Vec<Vec<(usize, f64)>> = vec![
        // norm warp, linear, nearest boundary, HSV
        vec![(3, 5.0), (5, -3.0), (7, 10.0), (9, -5.0), (11, 1.0), (12, 1.0), (13, 1.0), (17, 0.0),
             (20, 1.2), (22, 1.5), (24, 0.9), (28, 10.0), (30, -20.0), (19, 0.0)],
        // per-channel warp, nearest interpolation, periodic, HSI, 2 iterations
        vec![(1, 2.0), (3, 2.0), (5, 4.0), (7, -6.0), (9, 3.0), (11, 0.0), (12, 0.0), (13, 2.0), (17, 0.0),
             (20, 0.8), (22, 1.1), (24, 1.0), (28, 30.0), (30, 5.0), (19, 1.0)],
        // red channel only, mirror, HSL, HSX only at the end
        vec![(3, 4.0), (5, 4.0), (11, 1.0), (12, 1.0), (13, 3.0), (14, 4.0), (17, 0.0), (18, 0.0),
             (20, 1.0), (22, 1.0), (24, 1.0), (19, 2.0)],
    ];
    for case in cases {
        let ov: Vec<(usize, f64)> = fixed.iter().cloned().chain(case.iter().cloned()).collect();
        let (g, c) = compare("fx_ultrawarptwo", &ov, &f).unwrap();
        let p = g.psnr(&c);
        let pb = blur(&g, 1.5).psnr(&blur(&c, 1.5));
        eprintln!("ultrawarp2 {case:?}: {p:.1} dB (blurred {pb:.1} dB)");
        if std::env::var("GV_DUMP").is_ok() {
            let _ = g.write_ppm(std::path::Path::new(&format!("/tmp/uw_gpu_{}.ppm", case.len())));
            let _ = c.write_ppm(std::path::Path::new(&format!("/tmp/uw_gmic_{}.ppm", case.len())));
        }
        assert!(pb > 28.0, "ultrawarp2 {case:?}: {p:.1} dB, blurred {pb:.1} dB");
    }
}

#[test]
#[ignore]
fn ultrawarp2_isolate() {
    let f = test_frame(480).unwrap();
    let fixed: Vec<(usize, f64)> = [4, 6, 8, 10, 21, 23, 25, 27, 29, 31, 33, 35].iter().map(|&i| (i, 0.0)).collect();
    let base = vec![(3, 5.0), (5, -3.0), (7, 10.0), (9, -5.0), (11, 1.0), (12, 1.0), (13, 1.0), (17, 0.0),
        (20, 1.2), (22, 1.5), (24, 0.9), (28, 10.0), (30, -20.0), (19, 0.0)];
    for (name, change) in [("per-channel", (11, 0.0)), ("nearest", (12, 0.0)), ("periodic", (13, 2.0)),
        ("mirror", (13, 3.0)), ("hsi", (19, 1.0)), ("hsl", (19, 2.0)), ("2 iter", (1, 2.0)), ("red only", (14, 4.0))] {
        let mut ov: Vec<(usize, f64)> = fixed.iter().cloned().chain(base.iter().cloned()).collect();
        ov.push(change);
        let (g, c) = compare("fx_ultrawarptwo", &ov, &f).unwrap();
        eprintln!("{name}: {:.1} dB", g.psnr(&c));
    }
}

#[test]
fn ultrawarp2_speed_1080p() {
    need_gpu!();
    let Some(f) = test_frame(1920) else { return };
    let f = crate::frame::Frame { width: 1920, height: 1080, data: {
        // crop the square test image to 1080 rows
        let n = 1920 * f.height as usize;
        let mut d = Vec::with_capacity(1920 * 1080 * 3);
        for c in 0..3 { d.extend_from_slice(&f.data[c * n..c * n + 1920 * 1080]); }
        d
    } };
    let info = find("fx_ultrawarptwo").unwrap();
    let mut gpu = GpuProcessor::from_info(info).unwrap();
    let db = FilterDatabase::load_default();
    let filter = db.filters.iter().find(|f| f.command == "fx_ultrawarptwo").unwrap();
    let args = crate::gmic::argument_values(filter, &HashMap::new());
    gpu.process_args(&f, &args, 0).unwrap();
    let t = std::time::Instant::now();
    for i in 0..10 {
        gpu.process_args(&f, &args, i).unwrap();
    }
    eprintln!("ultrawarp2 GPU 1920x1080 (defaults, 3 iterations): {:.1} ms/frame", t.elapsed().as_secs_f64() * 100.0);
}

#[test]
fn ultrawarp3_matches_gmic_with_fixed_seed() {
    need_gpu!();
    let f = test_frame(480).unwrap();
    // No ranges -> the warp is fully determined by the centre values.
    for case in [
        vec![(1, 7.0), (2, 1.0), (3, 2.0), (4, 0.0), (5, 30.0), (6, 0.0), (7, 5.0), (8, 0.0), (9, 120.0), (10, 0.0)],
        vec![(1, 7.0), (2, 3.0), (3, 1.5), (4, 0.0), (5, 200.0), (6, 0.0), (7, 2.0), (8, 0.0), (9, 10.0), (10, 0.0), (11, 1.0)],
        vec![(1, 7.0), (2, 2.0), (3, 2.5), (4, 0.0), (5, 90.0), (6, 0.0), (8, 0.0), (10, 0.0), (15, 3.0)],
    ] {
        let (g, c) = compare("fx_ultrawarp3_preview", &case, &f).unwrap();
        let p = g.psnr(&c);
        eprintln!("ultrawarp3 {case:?}: {p:.1} dB");
        assert!(p > 30.0, "ultrawarp3 {case:?}: {p:.1} dB");
    }
}
