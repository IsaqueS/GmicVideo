use crate::gmic::{execute_gmic_on_bytes, FilterDatabase};
use crate::tween::ParameterTween;
use crate::video::{
    probe_video, read_png_frame, spawn_video_decoder, spawn_video_streaming_encoder, ExportFormat,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;

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
    pub base_values: HashMap<usize, f64>,
    pub tweens: HashMap<usize, ParameterTween>,
    #[serde(default)]
    pub cut_start_secs: Option<f64>,
    #[serde(default)]
    pub cut_end_secs: Option<f64>,
}

pub struct BatchRenderer {
    pub cancel_flag: Arc<AtomicBool>,
}

impl Default for BatchRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl BatchRenderer {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    /// Renders video frames on-the-fly and streams directly into FFmpeg encoder via pipes.
    /// Uses 0 bytes of disk space for intermediate frames, avoiding disk quota errors.
    pub fn run<F>(
        &self,
        request: RenderJobRequest,
        filter_db: &FilterDatabase,
        on_progress: F,
    ) -> Result<PathBuf, String>
    where
        F: Fn(RenderProgress) + Send + Sync + 'static,
    {
        self.cancel_flag.store(false, Ordering::SeqCst);
        let start_time = Instant::now();
        let on_progress = Arc::new(on_progress);

        // 1. Find filter
        let filter = filter_db
            .find_filter(&request.filter_id)
            .ok_or_else(|| format!("Filter not found: {}", request.filter_id))?
            .clone();

        // 2. Probe video
        let video_info = probe_video(&request.video_path)?;

        let effective_duration = match (request.cut_start_secs, request.cut_end_secs) {
            (Some(s), Some(e)) if e > s => e - s,
            (Some(s), None) => (video_info.duration_secs - s).max(0.0),
            (None, Some(e)) => e.min(video_info.duration_secs),
            _ => video_info.duration_secs,
        };
        let estimated_frames = ((effective_duration * video_info.fps).round() as usize).max(1);

        on_progress(RenderProgress {
            stage: RenderStage::ProcessingFrames,
            total_frames: estimated_frames,
            completed_frames: 0,
            percent: 2.0,
            fps: 0.0,
            elapsed_secs: 0.0,
            eta_secs: 0.0,
            message: "Streaming frames directly to ffmpeg (0 disk space)...".to_string(),
        });

        // 3. Setup multi-threaded pipeline channels
        let num_workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 16);
        let queue_bound = (num_workers * 2).clamp(8, 32);

        let cancel_flag = self.cancel_flag.clone();
        let pipeline_error = Arc::new(Mutex::new(Option::<String>::None));

        // 4. Spawn decoder subprocess
        let mut decoder_child = spawn_video_decoder(
            &request.video_path,
            request.cut_start_secs,
            request.cut_end_secs,
        )?;
        let decoder_stdout = decoder_child
            .stdout
            .take()
            .ok_or("Failed to open decoder stdout")?;

        // 5. Spawn streaming encoder subprocess
        let mut encoder_child = spawn_video_streaming_encoder(
            &request.video_path,
            &request.output_path,
            request.format,
            video_info.fps,
            request.crf,
            video_info.has_audio,
            &request.speed_preset,
            request.cut_start_secs,
            request.cut_end_secs,
        )?;
        let mut encoder_stdin = encoder_child
            .stdin
            .take()
            .ok_or("Failed to open encoder stdin")?;
        let mut decoder_stderr = decoder_child.stderr.take();
        let decoder_err_handle = std::thread::spawn(move || {
            let mut err_str = String::new();
            if let Some(ref mut r) = decoder_stderr {
                use std::io::Read;
                let _ = r.read_to_string(&mut err_str);
            }
            err_str
        });

        let mut encoder_stderr = encoder_child.stderr.take();
        let encoder_err_handle = std::thread::spawn(move || {
            let mut err_str = String::new();
            if let Some(ref mut r) = encoder_stderr {
                use std::io::Read;
                let _ = r.read_to_string(&mut err_str);
            }
            err_str
        });

        // 6. Decoder thread: reads PNG frames from decoder stdout into work_tx
        let (work_tx, work_rx) = mpsc::sync_channel::<(usize, Vec<u8>)>(queue_bound);
        let dec_cancel = cancel_flag.clone();
        let dec_err = pipeline_error.clone();

        let decoder_thread = std::thread::spawn(move || {
            let mut reader = BufReader::with_capacity(512 * 1024, decoder_stdout);
            let mut idx = 0;
            loop {
                if dec_cancel.load(Ordering::Relaxed) {
                    break;
                }
                match read_png_frame(&mut reader) {
                    Ok(Some(frame_bytes)) => {
                        if work_tx.send((idx, frame_bytes)).is_err() {
                            break;
                        }
                        idx += 1;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let mut err_guard = dec_err.lock().unwrap();
                        if err_guard.is_none() {
                            *err_guard = Some(format!("Decoder error: {e}"));
                        }
                        dec_cancel.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            drop(work_tx);
        });

        // 7. Worker pool: processes G'MIC filters in parallel with tweened params
        let (res_tx, res_rx) = mpsc::sync_channel::<(usize, Result<Vec<u8>, String>)>(queue_bound);
        let work_rx = Arc::new(Mutex::new(work_rx));
        let filter_arc = Arc::new(filter);
        let base_values = Arc::new(request.base_values.clone());
        let tweens = Arc::new(request.tweens.clone());
        let mut worker_handles = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let rx = work_rx.clone();
            let tx = res_tx.clone();
            let cancel = cancel_flag.clone();
            let filter_c = filter_arc.clone();
            let base_vals = base_values.clone();
            let tweens_c = tweens.clone();

            let handle = std::thread::spawn(move || {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let work_item = {
                        let rx_guard = rx.lock().unwrap();
                        rx_guard.recv()
                    };
                    let (idx, in_bytes) = match work_item {
                        Ok(item) => item,
                        Err(_) => break,
                    };

                    let t = if estimated_frames > 1 {
                        (idx as f64 / (estimated_frames - 1) as f64).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    let mut frame_values: HashMap<usize, f64> = (*base_vals).clone();
                    for (param_id, tween) in tweens_c.iter() {
                        let is_active = match tween.mode {
                            crate::tween::AnimationMode::Disabled => tween.enabled,
                            _ => true,
                        };
                        if is_active {
                            let (pmin, pmax) = filter_c
                                .parameters
                                .iter()
                                .find(|p| p.id == *param_id)
                                .map(|p| match p.param_type {
                                    crate::gmic::GmicParamType::Float { min, max, .. } => (min, max),
                                    crate::gmic::GmicParamType::Int { min, max, .. } => {
                                        (min as f64, max as f64)
                                    }
                                    _ => (-1e9, 1e9),
                                })
                                .unwrap_or((-1e9, 1e9));

                            let evaluated_val = tween.evaluate_at(t, idx, pmin, pmax);
                            frame_values.insert(*param_id, evaluated_val);
                        }
                    }

                    let render_res = execute_gmic_on_bytes(&in_bytes, &filter_c, &frame_values);
                    if tx.send((idx, render_res)).is_err() {
                        break;
                    }
                }
            });
            worker_handles.push(handle);
        }
        drop(res_tx);

        // 8. Real-time progress watcher thread
        let completed_counter = Arc::new(AtomicUsize::new(0));
        let progress_completed = completed_counter.clone();
        let watcher_cancel = Arc::new(AtomicBool::new(false));
        let watcher_cancel_clone = watcher_cancel.clone();
        let watcher_cb = on_progress.clone();

        let watcher_handle = std::thread::spawn(move || {
            while !watcher_cancel_clone.load(Ordering::Relaxed) {
                let done = progress_completed.load(Ordering::Relaxed);
                let elapsed = start_time.elapsed().as_secs_f64();
                let fps = if elapsed > 0.0 { done as f64 / elapsed } else { 0.0 };
                let total = estimated_frames.max(done);
                let remaining = total.saturating_sub(done);
                let eta = if fps > 0.05 { remaining as f64 / fps } else { 0.0 };
                let percent = if total > 0 {
                    ((done as f64 / total as f64) * 92.0).min(92.0)
                } else {
                    0.0
                };

                let prog = RenderProgress {
                    stage: RenderStage::ProcessingFrames,
                    total_frames: total,
                    completed_frames: done,
                    percent,
                    fps,
                    elapsed_secs: elapsed,
                    eta_secs: eta,
                    message: format!("Rendered {} / {} frames ({:.1} fps)", done, total, fps),
                };

                watcher_cb(prog);
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });

        // 9. Sequencer: writes processed frames sequentially (0, 1, 2, ...) to ffmpeg encoder stdin
        let mut pending: HashMap<usize, Vec<u8>> = HashMap::new();
        let mut next_expected = 0;
        

        while let Ok((idx, frame_res)) = res_rx.recv() {
            if cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            match frame_res {
                Ok(out_bytes) => {
                    pending.insert(idx, out_bytes);
                    while let Some(bytes) = pending.remove(&next_expected) {
                        if let Err(e) = encoder_stdin.write_all(&bytes) {
                            let mut err_guard = pipeline_error.lock().unwrap();
                            if err_guard.is_none() {
                                *err_guard = Some(format!("Failed to write to ffmpeg encoder: {e}"));
                            }
                            cancel_flag.store(true, Ordering::Relaxed);
                            break;
                        }
                        next_expected += 1;
                        completed_counter.fetch_add(1, Ordering::Relaxed);
                    }
                    if cancel_flag.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Err(e) => {
                    let mut err_guard = pipeline_error.lock().unwrap();
                    if err_guard.is_none() {
                        *err_guard = Some(e);
                    }
                    cancel_flag.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }

        // 10. Frame processing complete: stop watcher thread and close encoder stdin
        watcher_cancel.store(true, Ordering::Relaxed);
        let _ = watcher_handle.join();

        // Close encoder stdin so ffmpeg knows all video frames were received
        drop(encoder_stdin);

        // Clean up child processes if canceled or errored
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = decoder_child.kill();
            let _ = encoder_child.kill();
        }

        let _ = decoder_thread.join();
        for h in worker_handles {
            let _ = h.join();
        }
        let _ = decoder_child.wait();
        let _ = decoder_err_handle.join();

        // 11. Finalize encoder
        let final_count = completed_counter.load(Ordering::Relaxed);
        let format_name = request.format.display_name().to_string();

        on_progress(RenderProgress {
            stage: RenderStage::Encoding,
            total_frames: final_count,
            completed_frames: final_count,
            percent: 95.0,
            fps: 0.0,
            elapsed_secs: start_time.elapsed().as_secs_f64(),
            eta_secs: 0.0,
            message: format!("Finalizing {} video & audio multiplexing...", format_name),
        });

        let enc_status = encoder_child
            .wait()
            .map_err(|e| format!("Failed to wait on ffmpeg encoder: {e}"))?;
        let stderr_msg = encoder_err_handle.join().unwrap_or_default();

        if cancel_flag.load(Ordering::SeqCst) {
            if let Some(err) = pipeline_error.lock().unwrap().take() {
                return Err(err);
            }
            return Err("Render canceled by user".to_string());
        }

        if !enc_status.success() || !request.output_path.exists() {
            return Err(format!("ffmpeg encoding failed: {stderr_msg}"));
        }

        // 12. Completed!
        let total_time = start_time.elapsed().as_secs_f64();
        let fps = if total_time > 0.0 {
            final_count as f64 / total_time
        } else {
            0.0
        };

        on_progress(RenderProgress {
            stage: RenderStage::Completed,
            total_frames: final_count,
            completed_frames: final_count,
            percent: 100.0,
            fps,
            elapsed_secs: total_time,
            eta_secs: 0.0,
            message: format!("Render completed in {:.1}s ({:.1} fps)!", total_time, fps),
        });

        Ok(request.output_path)
    }
}
