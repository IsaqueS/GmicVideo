//! Final video render: decode → G'MIC (parallel workers) → encode, all streamed through pipes.
//!
//! Cancellation/error handling: a watchdog thread kills every child process (ffmpeg decoder,
//! encoder and the G'MIC workers) as soon as the job is canceled or fails. That unblocks any
//! thread stuck in a pipe read/write, and all channel operations use timeouts, so `run()`
//! always returns.

use crate::effect::{EffectSettings, PreparedEffect, Timeline};
use crate::frame::Frame;
use crate::gmic::FilterDatabase;
use crate::gmic_worker::{GmicWorker, WorkerKillHandle};
use crate::gpu::processor::GpuProcessor;
use crate::playback::FrameProcessor;
use crate::tween::ParameterTween;
use crate::video::{probe_video, spawn_decoder, spawn_encoder, DecoderOptions, EncoderOptions, ExportFormat};
use crossbeam_channel::{bounded, RecvTimeoutError, SendTimeoutError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(50);
/// GPU processors used by a render (frames overlap CPU prep/readback with GPU work).
const GPU_RENDER_PROCESSORS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderStage {
    Idle,
    Extracting,
    ProcessingFrames,
    Encoding,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderProgress {
    pub stage: RenderStage,
    pub total_frames: usize,
    pub completed_frames: usize,
    pub percent: f64,
    pub fps: f64,
    pub elapsed_secs: f64,
    pub eta_secs: f64,
    pub message: String,
}

impl Default for RenderProgress {
    fn default() -> Self {
        Self {
            stage: RenderStage::Idle,
            total_frames: 0,
            completed_frames: 0,
            percent: 0.0,
            fps: 0.0,
            elapsed_secs: 0.0,
            eta_secs: 0.0,
            message: "Ready".to_string(),
        }
    }
}

fn default_job_id() -> String {
    format!(
        "job_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}

fn default_job_name() -> String {
    "Video Render".to_string()
}

fn default_crf() -> u32 {
    22
}

fn default_speed_preset() -> String {
    "medium".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderJobRequest {
    #[serde(default = "default_job_id")]
    pub id: String,
    #[serde(default = "default_job_name")]
    pub name: String,
    pub video_path: PathBuf,
    pub output_path: PathBuf,
    pub format: ExportFormat,
    #[serde(default = "default_crf")]
    pub crf: u32,
    #[serde(default = "default_speed_preset")]
    pub speed_preset: String,
    pub filter_id: String,
    #[serde(default)]
    pub filter_name: String,
    #[serde(default)]
    pub base_values: HashMap<usize, f64>,
    #[serde(default)]
    pub tweens: HashMap<usize, ParameterTween>,
    #[serde(default)]
    pub text_values: HashMap<usize, String>,
    #[serde(default)]
    pub cut_start_secs: Option<f64>,
    #[serde(default)]
    pub cut_end_secs: Option<f64>,
}

impl RenderJobRequest {
    pub fn effect_settings(&self) -> EffectSettings {
        EffectSettings {
            filter_id: self.filter_id.clone(),
            base_values: self.base_values.clone(),
            tweens: self.tweens.clone(),
            text_values: self.text_values.clone(),
        }
    }
}

/// Picks how many G'MIC worker processes to run and how many threads each may use.
/// Many single-threaded workers give the best throughput, but big frames need a lot of
/// memory per worker, so high resolutions use fewer workers with more threads each.
pub fn worker_layout(width: u32, height: u32) -> (usize, usize) {
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 64);
    let pixels = width as usize * height as usize;
    let max_workers = if pixels > 4_000_000 {
        4
    } else if pixels > 2_200_000 {
        8
    } else {
        16
    };
    let workers = cores.clamp(1, max_workers);
    let threads = (cores / workers).max(1);
    (workers, threads)
}

/// Kills all child processes of a job once; shared by the watchdog and the main thread.
#[derive(Default)]
struct ProcessKiller {
    children: Mutex<Vec<Arc<Mutex<Child>>>>,
    workers: Mutex<Vec<WorkerKillHandle>>,
}

impl ProcessKiller {
    fn kill_all(&self) {
        for c in self.children.lock().unwrap().iter() {
            let _ = c.lock().unwrap().kill();
        }
        for w in self.workers.lock().unwrap().iter() {
            w.kill();
        }
    }
}

pub struct BatchRenderer;

impl BatchRenderer {
    /// Renders the whole job. Returns the output path, or an error message
    /// (`"Render canceled by user"` when `cancel` was set).
    pub fn run<F>(
        request: RenderJobRequest,
        filter_db: &FilterDatabase,
        cancel: Arc<AtomicBool>,
        on_progress: F,
    ) -> Result<PathBuf, String>
    where
        F: Fn(RenderProgress) + Send + Sync + 'static,
    {
        let start_time = Instant::now();
        let on_progress = Arc::new(on_progress);

        let effect = Arc::new(PreparedEffect::new(filter_db, request.effect_settings())?);
        let info = probe_video(&request.video_path)?;
        let timeline = Timeline::new(info.duration_secs, info.fps, request.cut_start_secs, request.cut_end_secs);
        let total_frames = timeline.total_frames();
        let (w, h) = (info.width, info.height);

        on_progress(RenderProgress {
            stage: RenderStage::Extracting,
            total_frames,
            percent: 1.0,
            message: "Starting G'MIC workers...".to_string(),
            ..Default::default()
        });

        // 1. Frame processors: the GPU version of the effect when available, otherwise G'MIC
        //    workers (started first so a broken G'MIC install fails fast).
        let killer = Arc::new(ProcessKiller::default());
        let mut workers: Vec<Box<dyn FrameProcessor>> = Vec::new();
        if GpuProcessor::would_handle(&effect) {
            let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
            for _ in 0..GPU_RENDER_PROCESSORS {
                let Some(gpu) = GpuProcessor::for_effect(&effect) else { break };
                // Animated arguments may leave the GPU-supported range: keep a CPU worker ready.
                let gpu = if effect.is_animated() {
                    let worker = GmicWorker::spawn((cores / GPU_RENDER_PROCESSORS).max(1))?;
                    killer.workers.lock().unwrap().push(worker.kill_handle());
                    gpu.with_fallback(worker)
                } else {
                    gpu.fallback_threads(cores / GPU_RENDER_PROCESSORS)
                };
                workers.push(Box::new(gpu));
            }
        }
        if workers.is_empty() {
            let (num_workers, threads_per_worker) = worker_layout(w, h);
            for _ in 0..num_workers {
                let worker = GmicWorker::spawn(threads_per_worker)?;
                killer.workers.lock().unwrap().push(worker.kill_handle());
                workers.push(Box::new(worker));
            }
        }
        let num_workers = workers.len();

        // 2. FFmpeg decoder and encoder.
        let fps = info.fps_fraction();
        let mut decoder = spawn_decoder(&DecoderOptions {
            path: &request.video_path,
            start_secs: Some(timeline.start_secs),
            end_secs: Some(timeline.end_secs),
            fps: fps.clone(),
            width: w,
            height: h,
            source_width: w,
            source_height: h,
        })?;
        let decoder_stdout = decoder.stdout.take().ok_or("Failed to open decoder stdout")?;
        let decoder_stderr = decoder.stderr.take();

        let mut encoder = match spawn_encoder(&EncoderOptions {
            original_video: &request.video_path,
            out_path: &request.output_path,
            format: request.format,
            width: w,
            height: h,
            fps,
            crf: request.crf,
            has_audio: info.has_audio,
            speed_preset: &request.speed_preset,
            cut_start_secs: Some(timeline.start_secs).filter(|s| *s > 0.0),
            cut_end_secs: Some(timeline.end_secs).filter(|e| *e < info.duration_secs - 0.0005),
        }) {
            Ok(e) => e,
            Err(e) => {
                let _ = decoder.kill();
                let _ = decoder.wait();
                return Err(e);
            }
        };
        let encoder_stdin = encoder.stdin.take().ok_or("Failed to open encoder stdin")?;
        let encoder_stderr = encoder.stderr.take();

        let decoder = Arc::new(Mutex::new(decoder));
        let encoder = Arc::new(Mutex::new(encoder));
        killer.children.lock().unwrap().push(decoder.clone());
        killer.children.lock().unwrap().push(encoder.clone());

        let decoder_err_handle = spawn_stderr_reader(decoder_stderr);
        let encoder_err_handle = spawn_stderr_reader(encoder_stderr);

        // `stop` = cancel requested OR an error happened somewhere in the pipeline.
        let stop = Arc::new(AtomicBool::new(false));
        let pipeline_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let set_error = {
            let pipeline_error = pipeline_error.clone();
            let stop = stop.clone();
            move |msg: String| {
                let mut guard = pipeline_error.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(msg);
                }
                stop.store(true, Ordering::SeqCst);
            }
        };

        // 3. Watchdog: turns cancel/stop into killed processes.
        let finished = Arc::new(AtomicBool::new(false));
        let watchdog = {
            let (cancel, stop, finished, killer) = (cancel.clone(), stop.clone(), finished.clone(), killer.clone());
            std::thread::spawn(move || {
                while !finished.load(Ordering::SeqCst) {
                    if cancel.load(Ordering::SeqCst) {
                        stop.store(true, Ordering::SeqCst);
                    }
                    if stop.load(Ordering::SeqCst) {
                        killer.kill_all();
                        break;
                    }
                    std::thread::sleep(POLL);
                }
            })
        };

        // 4. Decoder thread → work queue.
        let queue_bound = (num_workers * 2).clamp(4, 32);
        let (work_tx, work_rx) = bounded::<(usize, Frame)>(queue_bound);
        let decoder_thread = {
            let stop = stop.clone();
            let set_error = set_error.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::with_capacity(1 << 22, decoder_stdout);
                let mut idx = 0usize;
                while !stop.load(Ordering::SeqCst) {
                    match Frame::read_gbrp(&mut reader, w, h) {
                        Ok(Some(frame)) => {
                            let mut item = (idx, frame);
                            loop {
                                match work_tx.send_timeout(item, POLL) {
                                    Ok(()) => break,
                                    Err(SendTimeoutError::Timeout(back)) => {
                                        if stop.load(Ordering::SeqCst) {
                                            return idx;
                                        }
                                        item = back;
                                    }
                                    Err(SendTimeoutError::Disconnected(_)) => return idx,
                                }
                            }
                            idx += 1;
                        }
                        Ok(None) => break,
                        Err(e) => {
                            if !stop.load(Ordering::SeqCst) {
                                set_error(format!("Video decoding failed: {e}"));
                            }
                            break;
                        }
                    }
                }
                idx
            })
        };

        // 5. Worker threads: work queue → G'MIC → result queue.
        let (res_tx, res_rx) = bounded::<(usize, Result<Frame, String>)>(queue_bound);
        let mut worker_handles = Vec::with_capacity(num_workers);
        for mut worker in workers {
            let (rx, tx, stop, effect) = (work_rx.clone(), res_tx.clone(), stop.clone(), effect.clone());
            worker_handles.push(std::thread::spawn(move || {
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let (idx, frame) = match rx.recv_timeout(POLL) {
                        Ok(item) => item,
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    let result = worker
                        .process(&frame, &effect, &timeline, idx)
                        .map_err(|e| format!("Frame {}: {e}", idx + 1));
                    let mut item = (idx, result);
                    loop {
                        match tx.send_timeout(item, POLL) {
                            Ok(()) => break,
                            Err(SendTimeoutError::Timeout(back)) => {
                                if stop.load(Ordering::SeqCst) {
                                    return;
                                }
                                item = back;
                            }
                            Err(SendTimeoutError::Disconnected(_)) => return,
                        }
                    }
                }
            }));
        }
        drop(work_rx);
        drop(res_tx);

        // 6. Progress reporter.
        let completed = Arc::new(AtomicUsize::new(0));
        let reporter = {
            let (completed, finished, cb) = (completed.clone(), finished.clone(), on_progress.clone());
            std::thread::spawn(move || {
                while !finished.load(Ordering::SeqCst) {
                    let done = completed.load(Ordering::Relaxed);
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let fps = if elapsed > 0.0 { done as f64 / elapsed } else { 0.0 };
                    let total = total_frames.max(done);
                    let eta = if fps > 0.05 { total.saturating_sub(done) as f64 / fps } else { 0.0 };
                    cb(RenderProgress {
                        stage: RenderStage::ProcessingFrames,
                        total_frames: total,
                        completed_frames: done,
                        percent: (2.0 + done as f64 / total.max(1) as f64 * 93.0).min(95.0),
                        fps,
                        elapsed_secs: elapsed,
                        eta_secs: eta,
                        message: format!("Rendered {done} / {total} frames ({fps:.1} fps)"),
                    });
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
        };

        // 7. Sequencer (this thread): write frames to the encoder in order.
        let mut encoder_stdin = std::io::BufWriter::with_capacity(1 << 22, encoder_stdin);
        let mut pending: HashMap<usize, Frame> = HashMap::new();
        let mut next_expected = 0usize;
        loop {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            match res_rx.recv_timeout(POLL) {
                Ok((idx, Ok(frame))) => {
                    pending.insert(idx, frame);
                    while let Some(frame) = pending.remove(&next_expected) {
                        if let Err(e) = frame.write_gbrp(&mut encoder_stdin) {
                            if !stop.load(Ordering::SeqCst) {
                                set_error(format!("Failed to write to ffmpeg encoder: {e}"));
                            }
                            break;
                        }
                        next_expected += 1;
                        completed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Ok((_, Err(e))) => {
                    if !stop.load(Ordering::SeqCst) {
                        set_error(e);
                    }
                    break;
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break, // all workers done
            }
        }
        drop(res_rx);

        // 8. Shut down threads. Everything polls `stop` or sees closed channels, so these joins return.
        let decoded = decoder_thread.join().unwrap_or(0);
        for h in worker_handles {
            let _ = h.join();
        }
        // Poll instead of a blocking wait so the watchdog can still lock and kill it.
        let _ = wait_child(&decoder);
        let decoder_stderr_text = decoder_err_handle.join().unwrap_or_default();

        let stopped = stop.load(Ordering::SeqCst);
        if !stopped && next_expected < decoded {
            // A frame went missing without an error (should not happen); report rather than hang.
            set_error(format!("Only {next_expected} of {decoded} frames were processed"));
        }
        if !stopped && decoded == 0 {
            set_error(format!(
                "No frames could be decoded from the video. {}",
                crate::gmic_worker::clean_stderr(&decoder_stderr_text)
            ));
        }

        let final_count = completed.load(Ordering::Relaxed);
        if !stop.load(Ordering::SeqCst) {
            on_progress(RenderProgress {
                stage: RenderStage::Encoding,
                total_frames: final_count,
                completed_frames: final_count,
                percent: 96.0,
                elapsed_secs: start_time.elapsed().as_secs_f64(),
                message: format!("Finalizing {} video & audio...", request.format.display_name()),
                ..Default::default()
            });
        }

        // Closing stdin tells ffmpeg the video is complete (it is killed instead if we stopped).
        let flush_result = encoder_stdin.flush();
        drop(encoder_stdin);
        if let Err(e) = flush_result {
            if !stop.load(Ordering::SeqCst) {
                set_error(format!("Failed to write to ffmpeg encoder: {e}"));
            }
        }
        let enc_status = wait_child(&encoder);
        let encoder_stderr_text = encoder_err_handle.join().unwrap_or_default();

        finished.store(true, Ordering::SeqCst);
        let _ = watchdog.join();
        let _ = reporter.join();

        if let Some(err) = pipeline_error.lock().unwrap().take() {
            remove_partial_output(&request.output_path);
            return Err(err);
        }
        if cancel.load(Ordering::SeqCst) {
            remove_partial_output(&request.output_path);
            return Err("Render canceled by user".to_string());
        }
        let encoded_ok = enc_status.map(|s| s.success()).unwrap_or(false);
        if !encoded_ok || !request.output_path.exists() {
            remove_partial_output(&request.output_path);
            return Err(format!(
                "ffmpeg encoding failed: {}",
                crate::gmic_worker::clean_stderr(&encoder_stderr_text)
            ));
        }

        let total_time = start_time.elapsed().as_secs_f64();
        let fps = if total_time > 0.0 { final_count as f64 / total_time } else { 0.0 };
        on_progress(RenderProgress {
            stage: RenderStage::Completed,
            total_frames: final_count,
            completed_frames: final_count,
            percent: 100.0,
            fps,
            elapsed_secs: total_time,
            eta_secs: 0.0,
            message: format!("Render completed in {total_time:.1}s ({fps:.1} fps)!"),
        });

        Ok(request.output_path)
    }
}

fn spawn_stderr_reader<R: Read + Send + 'static>(stream: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut r) = stream {
            let _ = r.read_to_string(&mut text);
        }
        text
    })
}

fn wait_child(child: &Arc<Mutex<Child>>) -> Option<std::process::ExitStatus> {
    loop {
        {
            let mut c = child.lock().unwrap();
            match c.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => {}
                Err(_) => return None,
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn remove_partial_output(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn make_clip(dir: &std::path::Path, secs: u32) -> Option<PathBuf> {
        let src = dir.join("clip.mp4");
        let ok = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=s=96x64:r=12:d={secs}"))
            .args(["-f", "lavfi", "-i"])
            .arg(format!("sine=d={secs}"))
            .args(["-shortest", "-pix_fmt", "yuv420p"])
            .arg(&src)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        ok.then_some(src)
    }

    fn job(src: &std::path::Path, out: &std::path::Path, filter_id: &str) -> RenderJobRequest {
        RenderJobRequest {
            id: "t".into(),
            name: "t".into(),
            video_path: src.to_path_buf(),
            output_path: out.to_path_buf(),
            format: ExportFormat::H264,
            crf: 30,
            speed_preset: "ultrafast".into(),
            filter_id: filter_id.into(),
            filter_name: String::new(),
            base_values: HashMap::new(),
            tweens: HashMap::new(),
            text_values: HashMap::new(),
            cut_start_secs: None,
            cut_end_secs: None,
        }
    }

    /// Runs `f` on a thread and fails the test if it does not finish in `secs` (detects hangs).
    fn with_timeout<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(secs)).expect("render hung")
    }

    #[test]
    fn render_succeeds_with_cut() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 2) else { return };
        let out = dir.path().join("out.mp4");
        let mut req = job(&src, &out, "Vignette");
        req.cut_start_secs = Some(0.5);
        req.cut_end_secs = Some(1.5);
        let db = FilterDatabase::load_default();
        let res = with_timeout(120, move || BatchRenderer::run(req, &db, Arc::new(AtomicBool::new(false)), |_| {}));
        let path = res.expect("render should succeed");
        let info = probe_video(&path).unwrap();
        assert_eq!((info.width, info.height), (96, 64));
        assert!((info.duration_secs - 1.0).abs() < 0.25, "duration {}", info.duration_secs);
    }

    #[test]
    fn gpu_render_matches_cpu_render() {
        if !crate::gpu::GpuContext::is_available() {
            eprintln!("no GPU; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 1) else { return };
        let db = Arc::new(FilterDatabase::load_default());
        let render = |name: &str| {
            let out = dir.path().join(name);
            let req = job(&src, &out, "Vignette");
            let db = db.clone();
            with_timeout(120, move || BatchRenderer::run(req, &db, Arc::new(AtomicBool::new(false)), |_| {}))
                .expect("render should succeed")
        };
        let gpu = render("gpu.mp4");
        // The same filter forced through G'MIC: a copy of the filter under a name without a GPU version.
        let cpu = {
            let mut db2 = FilterDatabase::load_default();
            let mut f = db2.find_filter("Vignette").unwrap().clone();
            f.id = "vignette_cpu_test".into();
            f.name = "Vignette CPU test".into();
            f.command = format!("{} ", f.command); // not matched by the GPU registry
            db2.filters.insert(0, f);
            let out = dir.path().join("cpu.mp4");
            let req = job(&src, &out, "vignette_cpu_test");
            with_timeout(120, move || BatchRenderer::run(req, &db2, Arc::new(AtomicBool::new(false)), |_| {}))
                .expect("cpu render should succeed")
        };
        let a = crate::video::extract_frame(&probe_video(&gpu).unwrap(), 0.5, 96, 64).unwrap();
        let b = crate::video::extract_frame(&probe_video(&cpu).unwrap(), 0.5, 96, 64).unwrap();
        let p = a.psnr(&b);
        assert!(p > 30.0, "GPU vs CPU render PSNR {p:.1} dB");
    }

    #[test]
    fn failing_effect_returns_error_instead_of_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 3) else { return };
        let out = dir.path().join("out.mp4");
        let db = {
            let mut db = FilterDatabase::load_default();
            let mut broken = db.find_filter("Vignette").unwrap().clone();
            broken.id = "broken_test_filter".into();
            broken.name = "Broken".into();
            broken.command = "this_command_does_not_exist".into();
            db.filters.insert(0, broken);
            db
        };
        let req = job(&src, &out, "broken_test_filter");
        let res = with_timeout(60, move || BatchRenderer::run(req, &db, Arc::new(AtomicBool::new(false)), |_| {}));
        let err = res.unwrap_err();
        assert!(err.contains("this_command_does_not_exist"), "{err}");
        assert!(!out.exists(), "partial output must be removed");
    }

    #[test]
    fn cancel_returns_quickly() {
        let dir = tempfile::tempdir().unwrap();
        let Some(src) = make_clip(dir.path(), 20) else { return };
        let out = dir.path().join("out.mp4");
        let req = job(&src, &out, "Halftone");
        let db = FilterDatabase::load_default();
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            c2.store(true, Ordering::SeqCst);
        });
        let res = with_timeout(30, move || BatchRenderer::run(req, &db, cancel, |_| {}));
        assert_eq!(res.unwrap_err(), "Render canceled by user");
    }
}
