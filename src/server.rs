use crate::gmic::{execute_gmic_on_image, FilterDatabase};
use crate::renderer::{BatchRenderer, RenderJobRequest, RenderProgress, RenderStage};
use crate::tween::{evaluate_cubic_bezier, evaluate_curve_points, evaluate_ease, EaseType, ParameterTween};
use crate::video::{extract_preview_frame, probe_video};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Rendering,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderQueueItem {
    pub id: String,
    pub name: String,
    pub video_path: String,
    pub output_path: String,
    pub format: String,
    pub crf: u32,
    pub speed_preset: String,
    pub filter_id: String,
    pub filter_name: String,
    pub status: JobStatus,
    pub progress: RenderProgress,
    pub error: Option<String>,
    pub created_at_secs: f64,
}

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
    // New Queue Actions
    QueueRender(RenderJobRequest),
    GetRenderQueue,
    CancelRenderJob {
        job_id: String,
    },
    ClearCompletedJobs,
    // Legacy Single-Job Actions
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
    pub queue_items: Arc<Mutex<Vec<RenderQueueItem>>>,
    pub active_job_id: Arc<Mutex<Option<String>>>,
    pub preview_temp_dir: PathBuf,
    pub queue_tx: std::sync::mpsc::Sender<RenderJobRequest>,
    pub current_progress: Arc<Mutex<RenderProgress>>,
    pub is_rendering: Arc<Mutex<bool>>,
}

impl ServerState {
    pub fn new() -> Self {
        let temp_dir = std::env::temp_dir().join("gmic_video_previews");
        let _ = std::fs::create_dir_all(&temp_dir);

        let filter_db = FilterDatabase::load_default();
        let renderer = Arc::new(BatchRenderer::new());
        let queue_items: Arc<Mutex<Vec<RenderQueueItem>>> = Arc::new(Mutex::new(Vec::new()));
        let active_job_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let current_progress = Arc::new(Mutex::new(RenderProgress::default()));
        let is_rendering = Arc::new(Mutex::new(false));

        let (queue_tx, queue_rx) = std::sync::mpsc::channel::<RenderJobRequest>();

        // Background FIFO Queue Worker Thread
        let worker_queue = queue_items.clone();
        let worker_active_id = active_job_id.clone();
        let worker_renderer = renderer.clone();
        let worker_db = filter_db.clone();
        let worker_prog = current_progress.clone();
        let worker_is_rend = is_rendering.clone();

        std::thread::spawn(move || {
            while let Ok(job) = queue_rx.recv() {
                // Check if already canceled while in queue
                {
                    let items = worker_queue.lock().unwrap();
                    if let Some(item) = items.iter().find(|i| i.id == job.id) {
                        if item.status == JobStatus::Canceled {
                            continue;
                        }
                    }
                }

                // Mark as Rendering
                {
                    let mut items = worker_queue.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|i| i.id == job.id) {
                        item.status = JobStatus::Rendering;
                        item.progress.stage = RenderStage::Extracting;
                        item.progress.percent = 5.0;
                        item.progress.message = "Starting render...".to_string();
                    }
                    let mut active = worker_active_id.lock().unwrap();
                    *active = Some(job.id.clone());
                    let mut rend_guard = worker_is_rend.lock().unwrap();
                    *rend_guard = true;
                }

                let job_id = job.id.clone();
                let queue_clone = worker_queue.clone();
                let prog_clone = worker_prog.clone();

                let on_progress = move |p: RenderProgress| {
                    // Update queue item progress
                    let mut items = queue_clone.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|i| i.id == job_id) {
                        item.progress = p.clone();
                    }
                    // Update legacy progress
                    let mut guard = prog_clone.lock().unwrap();
                    *guard = p;
                };

                let result = worker_renderer.run(job.clone(), &worker_db, on_progress);

                // Update final status
                {
                    let mut items = worker_queue.lock().unwrap();
                    let mut active = worker_active_id.lock().unwrap();
                    *active = None;
                    let mut rend_guard = worker_is_rend.lock().unwrap();
                    *rend_guard = false;

                    if let Some(item) = items.iter_mut().find(|i| i.id == job.id) {
                        match result {
                            Ok(path) => {
                                item.status = JobStatus::Completed;
                                item.progress.stage = RenderStage::Completed;
                                item.progress.percent = 100.0;
                                item.progress.message = format!("Completed: {}", path.display());
                            }
                            Err(e) => {
                                if e.contains("canceled") || e.contains("Canceled") {
                                    item.status = JobStatus::Canceled;
                                    item.progress.stage = RenderStage::Canceled;
                                    item.progress.message = "Render was canceled".to_string();
                                } else {
                                    item.status = JobStatus::Failed;
                                    item.progress.stage = RenderStage::Failed;
                                    item.progress.message = format!("Failed: {e}");
                                    item.error = Some(e);
                                }
                            }
                        }
                    }
                }
            }
        });

        Self {
            filter_db,
            renderer,
            queue_items,
            active_job_id,
            preview_temp_dir: temp_dir,
            queue_tx,
            current_progress,
            is_rendering,
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
                let frame_idx = (time_secs * 30.0).round() as usize;
                for (param_id, tween) in tweens {
                    let is_active = match tween.mode {
                        crate::tween::AnimationMode::Disabled => tween.enabled,
                        _ => true,
                    };
                    if is_active {
                        let (pmin, pmax) = filter.parameters.iter()
                            .find(|p| p.id == param_id)
                            .map(|p| match p.param_type {
                                crate::gmic::GmicParamType::Float { min, max, .. } => (min, max),
                                crate::gmic::GmicParamType::Int { min, max, .. } => (min as f64, max as f64),
                                _ => (-1e9, 1e9),
                            })
                            .unwrap_or((-1e9, 1e9));

                        current_values.insert(param_id, tween.evaluate_at(t, frame_idx, pmin, pmax));
                    }
                }

                if let Err(e) = execute_gmic_on_image(&raw_frame_path, &filter, &current_values, &out_frame_path) {
                    return IpcResponse::Error(format!("Failed to apply G'MIC effect: {e}"));
                }

                let result = serde_json::json!({
                    "raw_preview": raw_frame_path.to_string_lossy(),
                    "effect_preview": out_frame_path.to_string_lossy(),
                    "time_secs": time_secs,
                    "normalized_t": t
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
            // Queue Render Action
            IpcRequest::QueueRender(mut job) => {
                if job.id.is_empty() {
                    job.id = format!(
                        "job_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis()
                    );
                }

                let filter_name = self
                    .filter_db
                    .find_filter(&job.filter_id)
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| job.filter_id.clone());

                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();

                let queue_item = RenderQueueItem {
                    id: job.id.clone(),
                    name: job.name.clone(),
                    video_path: job.video_path.to_string_lossy().to_string(),
                    output_path: job.output_path.to_string_lossy().to_string(),
                    format: job.format.display_name().to_string(),
                    crf: job.crf,
                    speed_preset: job.speed_preset.clone(),
                    filter_id: job.filter_id.clone(),
                    filter_name,
                    status: JobStatus::Queued,
                    progress: RenderProgress::default(),
                    error: None,
                    created_at_secs: now_secs,
                };

                {
                    let mut items = self.queue_items.lock().unwrap();
                    items.push(queue_item);
                }

                let _ = self.queue_tx.send(job.clone());
                IpcResponse::Ok(serde_json::json!({ "status": "queued", "job_id": job.id }))
            }
            IpcRequest::GetRenderQueue => {
                let items = self.queue_items.lock().unwrap();
                IpcResponse::Ok(serde_json::to_value(&*items).unwrap_or_default())
            }
            IpcRequest::CancelRenderJob { job_id } => {
                let mut was_active = false;
                {
                    let active = self.active_job_id.lock().unwrap();
                    if let Some(ref aid) = *active {
                        if aid == &job_id {
                            was_active = true;
                        }
                    }
                }

                if was_active {
                    self.renderer.cancel();
                } else {
                    let mut items = self.queue_items.lock().unwrap();
                    if let Some(item) = items.iter_mut().find(|i| i.id == job_id) {
                        item.status = JobStatus::Canceled;
                        item.progress.stage = RenderStage::Canceled;
                        item.progress.message = "Canceled in queue".into();
                    }
                }
                IpcResponse::Ok(serde_json::json!({ "status": "canceled", "job_id": job_id }))
            }
            IpcRequest::ClearCompletedJobs => {
                let mut items = self.queue_items.lock().unwrap();
                items.retain(|i| i.status == JobStatus::Queued || i.status == JobStatus::Rendering);
                IpcResponse::Ok(serde_json::json!({ "status": "cleared" }))
            }
            // Legacy single-job handlers
            IpcRequest::StartRender(job) => {
                let filter_name = self
                    .filter_db
                    .find_filter(&job.filter_id)
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| job.filter_id.clone());

                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();

                let queue_item = RenderQueueItem {
                    id: job.id.clone(),
                    name: job.name.clone(),
                    video_path: job.video_path.to_string_lossy().to_string(),
                    output_path: job.output_path.to_string_lossy().to_string(),
                    format: job.format.display_name().to_string(),
                    crf: job.crf,
                    speed_preset: job.speed_preset.clone(),
                    filter_id: job.filter_id.clone(),
                    filter_name,
                    status: JobStatus::Queued,
                    progress: RenderProgress::default(),
                    error: None,
                    created_at_secs: now_secs,
                };

                {
                    let mut items = self.queue_items.lock().unwrap();
                    items.push(queue_item);
                }

                let _ = self.queue_tx.send(job.clone());
                IpcResponse::Ok(serde_json::json!({ "status": "started", "job_id": job.id }))
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
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let req: Result<IpcRequest, _> = serde_json::from_str(line);
        let resp = match req {
            Ok(r) => state.handle_request(r),
            Err(e) => IpcResponse::Error(format!("Invalid JSON-IPC request: {e}")),
        };

        let resp_json = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
        let _ = writeln!(stdout, "{}", resp_json);
        let _ = stdout.flush();
    }
}
