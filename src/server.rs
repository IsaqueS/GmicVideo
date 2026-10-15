use crate::gmic::{execute_gmic_on_image, FilterDatabase};
use crate::renderer::{BatchRenderer, RenderJobRequest, RenderProgress, RenderStage};
use crate::tween::{evaluate_cubic_bezier, evaluate_curve_points, evaluate_ease, EaseType, ParameterTween};
use crate::video::{extract_preview_frame, probe_video};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "payload", rename_all = "snake_case")]
pub enum IpcRequest {
    GetEffects,
    ProbeVideo {
        path: String,
    },
    RenderPreview {
        video_path: String,
        time_secs: f64,
        duration_secs: f64,
        filter_id: String,
        base_values: HashMap<usize, f64>,
        tweens: HashMap<usize, ParameterTween>,
    },
    EvaluateCurve {
        ease_type: EaseType,
        bezier_points: Option<[f64; 4]>,
        curve_points: Option<Vec<(f64, f64)>>,
        sample_count: usize,
    },
    StartRender(RenderJobRequest),
    GetRenderProgress,
    CancelRender,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum IpcResponse {
    Ok(serde_json::Value),
    Error(String),
}

pub struct ServerState {
    pub filter_db: FilterDatabase,
    pub renderer: Arc<BatchRenderer>,
    pub current_progress: Arc<Mutex<RenderProgress>>,
    pub is_rendering: Arc<Mutex<bool>>,
    pub preview_temp_dir: PathBuf,
}

impl ServerState {
    pub fn new() -> Self {
        let temp_dir = std::env::temp_dir().join("gmic_video_previews");
        let _ = std::fs::create_dir_all(&temp_dir);

        Self {
            filter_db: FilterDatabase::load_default(),
            renderer: Arc::new(BatchRenderer::new()),
            current_progress: Arc::new(Mutex::new(RenderProgress::default())),
            is_rendering: Arc::new(Mutex::new(false)),
            preview_temp_dir: temp_dir,
        }
    }

    pub fn handle_request(&self, req: IpcRequest) -> IpcResponse {
        match req {
            IpcRequest::GetEffects => {
                let json = serde_json::to_value(&self.filter_db).unwrap_or_default();
                IpcResponse::Ok(json)
            }
            IpcRequest::ProbeVideo { path } => match probe_video(Path::new(&path)) {
                Ok(info) => IpcResponse::Ok(serde_json::to_value(info).unwrap_or_default()),
                Err(e) => IpcResponse::Error(e),
            },
            IpcRequest::RenderPreview {
                video_path,
                time_secs,
                duration_secs,
                filter_id,
                base_values,
                tweens,
            } => {
                let filter = match self.filter_db.find_filter(&filter_id) {
                    Some(f) => f.clone(),
                    None => return IpcResponse::Error(format!("Filter {filter_id} not found")),
                };

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();

                // Clean up previous preview files to keep disk clean
                if let Ok(entries) = std::fs::read_dir(&self.preview_temp_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.extension().map(|e| e == "png").unwrap_or(false) {
                            let _ = std::fs::remove_file(p);
                        }
                    }
                }

                let raw_frame_path = self.preview_temp_dir.join(format!("preview_raw_{}.png", now_ms));
                let out_frame_path = self.preview_temp_dir.join(format!("preview_effect_{}.png", now_ms));

                if let Err(e) = extract_preview_frame(Path::new(&video_path), time_secs, &raw_frame_path) {
                    return IpcResponse::Error(format!("Failed to extract preview frame: {e}"));
                }

                // Calculate normalized time
                let t = if duration_secs > 0.0 {
                    (time_secs / duration_secs).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let mut current_values = base_values;
                for (param_id, tween) in tweens {
                    if tween.enabled {
                        current_values.insert(param_id, tween.evaluate(t));
                    }
                }

                if let Err(e) = execute_gmic_on_image(&raw_frame_path, &filter, &current_values, &out_frame_path) {
                    return IpcResponse::Error(format!("Failed to apply G'MIC effect: {e}"));
                }

                let result = serde_json::json!({
                    "raw_preview": raw_frame_path.to_string_lossy(),
                    "effect_preview": out_frame_path.to_string_lossy(),
                    "time_secs": time_secs,
                    "normalized_t": t,
                });

                IpcResponse::Ok(result)
            }
            IpcRequest::EvaluateCurve {
                ease_type,
                bezier_points,
                curve_points,
                sample_count,
            } => {
                let n = sample_count.max(10);
                let mut points = Vec::with_capacity(n);

                for i in 0..=n {
                    let t = i as f64 / n as f64;
                    let y = match ease_type {
                        EaseType::CustomBezier => {
                            let pts = bezier_points.unwrap_or([0.25, 0.1, 0.25, 1.0]);
                            evaluate_cubic_bezier(t, pts[0], pts[1], pts[2], pts[3])
                        }
                        _ => {
                            if let Some(ref cpts) = curve_points {
                                if !cpts.is_empty() {
                                    evaluate_curve_points(t, cpts)
                                } else {
                                    evaluate_ease(t, ease_type)
                                }
                            } else {
                                evaluate_ease(t, ease_type)
                            }
                        }
                    };
                    points.push([t, y]);
                }

                IpcResponse::Ok(serde_json::to_value(points).unwrap_or_default())
            }
            IpcRequest::StartRender(job) => {
                let mut is_rend = self.is_rendering.lock().unwrap();
                if *is_rend {
                    return IpcResponse::Error("A render job is already running".to_string());
                }
                *is_rend = true;

                let renderer = self.renderer.clone();
                let filter_db = self.filter_db.clone();
                let prog_arc = self.current_progress.clone();
                let is_rend_arc = self.is_rendering.clone();

                std::thread::spawn(move || {
                    let prog_callback = {
                        let prog_arc = prog_arc.clone();
                        move |p: RenderProgress| {
                            let mut guard = prog_arc.lock().unwrap();
                            *guard = p;
                        }
                    };

                    let res = renderer.run(job, &filter_db, prog_callback);

                    let mut guard = prog_arc.lock().unwrap();
                    let mut is_rend_guard = is_rend_arc.lock().unwrap();
                    *is_rend_guard = false;

                    match res {
                        Ok(p) => {
                            guard.stage = RenderStage::Completed;
                            guard.percent = 100.0;
                            guard.message = format!("Render completed: {}", p.display());
                        }
                        Err(e) => {
                            if e.contains("canceled") || e.contains("Canceled") {
                                guard.stage = RenderStage::Canceled;
                                guard.message = "Render was canceled".into();
                            } else {
                                guard.stage = RenderStage::Failed;
                                guard.message = format!("Render failed: {e}");
                            }
                        }
                    }
                });

                IpcResponse::Ok(serde_json::json!({ "status": "started" }))
            }
            IpcRequest::GetRenderProgress => {
                let guard = self.current_progress.lock().unwrap();
                IpcResponse::Ok(serde_json::to_value(&*guard).unwrap_or_default())
            }
            IpcRequest::CancelRender => {
                self.renderer.cancel();
                let mut guard = self.current_progress.lock().unwrap();
                guard.stage = RenderStage::Canceled;
                guard.message = "Canceling render...".into();
                IpcResponse::Ok(serde_json::json!({ "status": "canceling" }))
            }
        }
    }
}

pub fn run_stdio_server() {
    let state = ServerState::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let resp = match serde_json::from_str::<IpcRequest>(trimmed) {
            Ok(req) => state.handle_request(req),
            Err(e) => IpcResponse::Error(format!("Failed to parse request JSON: {e}")),
        };

        if let Ok(json_str) = serde_json::to_string(&resp) {
            let _ = writeln!(stdout, "{}", json_str);
            let _ = stdout.flush();
        }
    }
}
