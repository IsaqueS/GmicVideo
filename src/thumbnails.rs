//! `--generate-thumbnails`: renders a small preview of every effect for the visual search.
//!
//! Each effect runs (with default parameters) on `Tests/test_image.jpeg` scaled to 256×256,
//! using parallel G'MIC workers with a per-effect timeout. Results are saved as
//! `<filter id>.jpg` plus a `thumbnails.json` manifest. Effects that fail, time out or produce
//! a flat image get no thumbnail (the visual search only shows effects with a preview).

use crate::frame::Frame;
use crate::gmic::{build_gmic_command, FilterDatabase, GmicFilter};
use crate::gmic_worker::GmicWorker;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const SIZE: u32 = 256;

#[derive(Debug, Clone, Serialize)]
pub struct ThumbnailResult {
    pub id: String,
    pub name: String,
    pub category: String,
    pub command: String,
    pub status: String,
    pub output_path: String,
    pub error: String,
    pub elapsed: f64,
}

pub fn default_output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/thumbnails")
}

fn source_image() -> Result<Frame, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Tests/test_image.jpeg");
    let info = crate::video::probe_video(&path)?;
    crate::video::extract_frame(&info, 0.0, SIZE, SIZE)
}

fn is_flat(f: &Frame) -> bool {
    let n = f.data.len() as f64;
    let mean = f.data.iter().map(|&v| v as f64).sum::<f64>() / n;
    let var = f.data.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / n;
    var.sqrt() < 1.0
}

fn save_jpeg(f: &Frame, path: &Path) -> Result<(), String> {
    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "gbrp", "-s"])
        .arg(format!("{}x{}", f.width, f.height))
        .args(["-i", "pipe:0", "-frames:v", "1", "-q:v", "3"])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("ffmpeg: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("ffmpeg stdin")?;
        let mut buf = Vec::with_capacity(f.data.len());
        f.write_gbrp(&mut buf).map_err(|e| e.to_string())?;
        stdin.write_all(&buf).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() && path.exists() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Renders thumbnails for all filters. `force` re-renders existing ones.
pub fn generate<F>(db: &FilterDatabase, out_dir: &Path, timeout: Duration, force: bool, on_result: F) -> Result<Vec<ThumbnailResult>, String>
where
    F: Fn(usize, usize, &ThumbnailResult) + Send + Sync,
{
    std::fs::create_dir_all(out_dir).map_err(|e| format!("Cannot create {}: {e}", out_dir.display()))?;
    let source = Arc::new(source_image()?);
    let filters: Vec<GmicFilter> = db.filters.clone();
    let total = filters.len();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(total));
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let workers = (cores / 2).clamp(1, 8);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let mut worker: Option<GmicWorker> = None;
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= total {
                        break;
                    }
                    let f = &filters[i];
                    let out_path = out_dir.join(format!("{}.jpg", f.id));
                    let started = Instant::now();
                    let mut r = ThumbnailResult {
                        id: f.id.clone(),
                        name: f.name.clone(),
                        category: f.category.clone(),
                        command: f.command.clone(),
                        status: "valid".into(),
                        output_path: out_path.to_string_lossy().to_string(),
                        error: String::new(),
                        elapsed: 0.0,
                    };
                    if !force && out_path.exists() {
                        r.status = "valid".into();
                    } else {
                        if worker.as_ref().map(|w| w.is_dead()).unwrap_or(true) {
                            worker = GmicWorker::spawn(2).ok();
                        }
                        match worker.as_mut() {
                            None => {
                                r.status = "error".into();
                                r.error = "G'MIC worker failed to start".into();
                            }
                            Some(w) => {
                                // Kill the worker if the effect takes too long.
                                let kill = w.kill_handle();
                                let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
                                let fin = finished.clone();
                                let watchdog = std::thread::spawn(move || {
                                    let t0 = Instant::now();
                                    while !fin.load(Ordering::SeqCst) {
                                        if t0.elapsed() > timeout {
                                            kill.kill();
                                            return true;
                                        }
                                        std::thread::sleep(Duration::from_millis(50));
                                    }
                                    false
                                });
                                let cmd = build_gmic_command(f, &HashMap::new(), &HashMap::new());
                                let res = w.process(&source, &cmd, 0);
                                finished.store(true, Ordering::SeqCst);
                                let timed_out = watchdog.join().unwrap_or(false);
                                match res {
                                    Ok(img) if is_flat(&img) => {
                                        r.status = "blank".into();
                                        r.error = "Effect produced a flat image".into();
                                    }
                                    Ok(img) => {
                                        if let Err(e) = save_jpeg(&img, &out_path) {
                                            r.status = "error".into();
                                            r.error = e;
                                        }
                                    }
                                    Err(e) => {
                                        r.status = if timed_out { "timeout".into() } else { "error".into() };
                                        r.error = if timed_out { format!("Timed out after {}s", timeout.as_secs()) } else { e };
                                    }
                                }
                            }
                        }
                    }
                    r.elapsed = started.elapsed().as_secs_f64();
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    on_result(n, total, &r);
                    results.lock().unwrap().push(r);
                }
            });
        }
    });

    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| a.id.cmp(&b.id));
    let manifest = out_dir.join("thumbnails.json");
    std::fs::write(&manifest, serde_json::to_string_pretty(&results).unwrap_or_default())
        .map_err(|e| format!("Cannot write {}: {e}", manifest.display()))?;
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_few_thumbnails() {
        let mut db = FilterDatabase::load_default();
        db.filters.retain(|f| ["Vignette", "Warhol"].contains(&f.name.as_str()));
        let mut broken = db.filters[0].clone();
        broken.id = "broken_thumb".into();
        broken.command = "this_command_does_not_exist".into();
        db.filters.push(broken);
        let dir = tempfile::tempdir().unwrap();
        let res = generate(&db, dir.path(), Duration::from_secs(30), true, |_, _, _| {}).unwrap();
        assert_eq!(res.len(), 3);
        for r in &res {
            if r.id == "broken_thumb" {
                assert_eq!(r.status, "error");
            } else {
                assert_eq!(r.status, "valid", "{r:?}");
                assert!(Path::new(&r.output_path).exists());
            }
        }
        assert!(dir.path().join("thumbnails.json").exists());
    }
}
