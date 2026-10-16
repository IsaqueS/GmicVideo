use crate::gmic::{execute_gmic_on_image, FilterDatabase};
use crate::tween::ParameterTween;
use crate::video::{encode_video, extract_all_frames, probe_video, ExportFormat};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
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

        // 3. Create temp workspace
        let temp_dir = tempfile::Builder::new()
            .prefix("gmic_video_render_")
            .tempdir()
            .map_err(|e| format!("Failed to create temp directory: {e}"))?;

        let in_frames_dir = temp_dir.path().join("in_frames");
        let out_frames_dir = temp_dir.path().join("out_frames");
        std::fs::create_dir_all(&in_frames_dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&out_frames_dir).map_err(|e| e.to_string())?;

        // 4. Extract frames
        on_progress(RenderProgress {
            stage: RenderStage::Extracting,
            total_frames: video_info.frame_count as usize,
            completed_frames: 0,
            percent: 5.0,
            fps: 0.0,
            elapsed_secs: start_time.elapsed().as_secs_f64(),
            eta_secs: 0.0,
            message: "Extracting video frames...".to_string(),
        });

        let total_extracted = extract_all_frames(&request.video_path, &in_frames_dir, request.cut_start_secs, request.cut_end_secs)?;
        if total_extracted == 0 {
            return Err("Zero frames extracted from video".to_string());
        }

        if self.cancel_flag.load(Ordering::SeqCst) {
            return Err("Render canceled by user".to_string());
        }

        // Collect all extracted frames sorted
        let mut frame_files: Vec<PathBuf> = std::fs::read_dir(&in_frames_dir)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|ent| ent.path()))
            .filter(|p| p.extension().map(|e| e == "png").unwrap_or(false))
            .collect();
        frame_files.sort();

        let num_frames = frame_files.len();
        let completed_counter = Arc::new(AtomicUsize::new(0));
        let cancel_flag = self.cancel_flag.clone();
        let base_values = Arc::new(request.base_values.clone());
        let tweens = Arc::new(request.tweens.clone());
        let filter_arc = Arc::new(filter);
        let process_start_time = Instant::now();

        // 5. Parallel frame processing with Rayon
        on_progress(RenderProgress {
            stage: RenderStage::ProcessingFrames,
            total_frames: num_frames,
            completed_frames: 0,
            percent: 10.0,
            fps: 0.0,
            elapsed_secs: start_time.elapsed().as_secs_f64(),
            eta_secs: 0.0,
            message: format!("Processing {} frames in parallel...", num_frames),
        });

        // Spawn a real-time progress reporting loop in background
        let progress_cancel = Arc::new(AtomicBool::new(false));
        let watcher_cancel = progress_cancel.clone();
        let watcher_counter = completed_counter.clone();
        let watcher_cb = on_progress.clone();

        let watcher_handle = std::thread::spawn(move || {
            while !watcher_cancel.load(Ordering::Relaxed) {
                let done = watcher_counter.load(Ordering::Relaxed);
                let elapsed = process_start_time.elapsed().as_secs_f64();
                let fps = if elapsed > 0.0 { done as f64 / elapsed } else { 0.0 };
                let remaining = num_frames.saturating_sub(done);
                let eta = if fps > 0.05 { remaining as f64 / fps } else { 0.0 };
                let percent = 10.0 + (done as f64 / num_frames as f64) * 78.0;

                let prog = RenderProgress {
                    stage: RenderStage::ProcessingFrames,
                    total_frames: num_frames,
                    completed_frames: done,
                    percent,
                    fps,
                    elapsed_secs: elapsed,
                    eta_secs: eta,
                    message: format!("Rendered {} / {} frames ({:.1} fps)", done, num_frames, fps),
                };

                watcher_cb(prog);

                if done >= num_frames {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });

        // Rayon parallel map over all frames
        let render_result: Result<(), String> = frame_files
            .par_iter()
            .enumerate()
            .try_for_each(|(idx, in_path)| {
                if cancel_flag.load(Ordering::Relaxed) {
                    return Err("Canceled".to_string());
                }

                // Compute normalized time t in [0.0, 1.0]
                let t = if num_frames > 1 {
                    idx as f64 / (num_frames - 1) as f64
                } else {
                    0.0
                };

                // Compute parameter values for this frame
                let mut frame_values: HashMap<usize, f64> = (*base_values).clone();

                for (param_id, tween) in tweens.iter() {
                    let is_active = match tween.mode {
                        crate::tween::AnimationMode::Disabled => tween.enabled,
                        _ => true,
                    };
                    if is_active {
                        let (pmin, pmax) = filter_arc.parameters.iter()
                            .find(|p| p.id == *param_id)
                            .map(|p| match p.param_type {
                                crate::gmic::GmicParamType::Float { min, max, .. } => (min, max),
                                crate::gmic::GmicParamType::Int { min, max, .. } => (min as f64, max as f64),
                                _ => (-1e9, 1e9),
                            })
                            .unwrap_or((-1e9, 1e9));

                        let evaluated_val = tween.evaluate_at(t, idx, pmin, pmax);
                        frame_values.insert(*param_id, evaluated_val);
                    }
                }

                let out_filename = format!("frame_{:07}.png", idx + 1);
                let out_path = out_frames_dir.join(out_filename);

                execute_gmic_on_image(in_path, &filter_arc, &frame_values, &out_path)?;

                completed_counter.fetch_add(1, Ordering::Relaxed);
                Ok(())
            });

        // Stop watcher thread
        progress_cancel.store(true, Ordering::Relaxed);
        let _ = watcher_handle.join();

        if let Err(e) = render_result {
            return Err(e);
        }

        if self.cancel_flag.load(Ordering::SeqCst) {
            return Err("Render canceled by user".to_string());
        }

        // 6. Encode video with ffmpeg
        let enc_on_progress = on_progress.clone();
        let enc_start_time = std::time::Instant::now();
        let format_name = request.format.display_name().to_string();

        on_progress(RenderProgress {
            stage: RenderStage::Encoding,
            total_frames: num_frames,
            completed_frames: 0,
            percent: 90.0,
            fps: 0.0,
            elapsed_secs: start_time.elapsed().as_secs_f64(),
            eta_secs: 0.0,
            message: format!("Starting {} encoding...", format_name),
        });

        encode_video(
            &out_frames_dir,
            &request.video_path,
            &request.output_path,
            request.format,
            video_info.fps,
            request.crf,
            video_info.has_audio,
            &request.speed_preset,
            request.cut_start_secs,
            request.cut_end_secs,
            Some(|enc_frame: usize, enc_fps: f64| {
                let enc_frac = if num_frames > 0 {
                    (enc_frame as f64 / num_frames as f64).min(1.0)
                } else {
                    1.0
                };
                let percent = 90.0 + enc_frac * 9.5;
                let elapsed = start_time.elapsed().as_secs_f64();
                let enc_elapsed = enc_start_time.elapsed().as_secs_f64();
                let remaining_frames = num_frames.saturating_sub(enc_frame);
                let eta = if enc_fps > 0.05 {
                    remaining_frames as f64 / enc_fps
                } else if enc_frac > 0.01 {
                    (enc_elapsed / enc_frac) - enc_elapsed
                } else {
                    0.0
                };

                enc_on_progress(RenderProgress {
                    stage: RenderStage::Encoding,
                    total_frames: num_frames,
                    completed_frames: enc_frame.min(num_frames),
                    percent,
                    fps: enc_fps,
                    elapsed_secs: elapsed,
                    eta_secs: eta.max(0.0),
                    message: format!(
                        "Encoding {} video: {} / {} frames ({:.1} fps)",
                        format_name,
                        enc_frame.min(num_frames),
                        num_frames,
                        enc_fps
                    ),
                });
            }),
        )?;

        // 7. Completed!
        let total_time = start_time.elapsed().as_secs_f64();
        on_progress(RenderProgress {
            stage: RenderStage::Completed,
            total_frames: num_frames,
            completed_frames: num_frames,
            percent: 100.0,
            fps: num_frames as f64 / total_time,
            elapsed_secs: total_time,
            eta_secs: 0.0,
            message: format!("Render completed in {:.1}s!", total_time),
        });

        Ok(request.output_path)
    }
}
